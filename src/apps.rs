//! Desktop application lookup and process grouping. Runs on the sampler thread.
use crate::metrics::Process;
use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

// A process's group key and the desktop app it belongs to, if any.
type Membership = (String, Option<Arc<App>>);
#[derive(Debug)]
pub struct App {
    pub name: String,
    pub icon: Option<PathBuf>,
}
#[derive(Clone, Default)]
struct Entry {
    name: String,
    icon: String,
}
/// Desktop entries indexed by lowercase desktop ID, plus executable and window-class aliases.
pub struct Apps {
    data: Vec<PathBuf>,
    stamps: Vec<Option<SystemTime>>,
    entries: HashMap<String, Entry>,
    aliases: HashMap<String, String>,
    weak: HashMap<String, String>,
    resolved: HashMap<String, Option<Arc<App>>>,
}
// Launchers whose basename says nothing about the application they start.
const WRAPPERS: [&str; 9] = [
    "sh", "bash", "env", "flatpak", "python", "python3", "java", "gjs", "electron",
];
const SIZES: [&str; 10] = [
    "scalable", "48x48", "64x64", "128x128", "256x256", "96x96", "32x32", "512x512", "24x24",
    "16x16",
];

fn data_dirs() -> Vec<PathBuf> {
    let home = env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let mut dirs = vec![
        env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share")),
    ];
    dirs.extend(
        env::var("XDG_DATA_DIRS")
            .unwrap_or("/usr/local/share:/usr/share".into())
            .split(':')
            .filter(|d| !d.is_empty())
            .map(PathBuf::from),
    );
    dirs.extend([
        home.join(".local/share/flatpak/exports/share"),
        "/var/lib/flatpak/exports/share".into(),
        "/var/lib/snapd/desktop".into(),
    ]);
    let mut seen = vec![];
    dirs.retain(|d| {
        let new = !seen.contains(d);
        seen.push(d.clone());
        new
    });
    dirs
}
fn basename(s: &str) -> &str {
    s.rsplit('/').next().unwrap_or(s)
}
// The program an Exec line starts, skipping `env VAR=value` prefixes and generic launchers.
fn exec_name(exec: &str) -> Option<String> {
    let program = exec
        .split_whitespace()
        .map(|t| t.trim_matches('"'))
        .find(|t| *t != "env" && !t.contains('='))?;
    let name = basename(program).to_lowercase();
    (!name.is_empty() && !WRAPPERS.contains(&name.as_str())).then_some(name)
}
fn parse_entry(text: &str) -> Option<(Entry, bool, Vec<String>)> {
    let (mut section, mut entry, mut visible, mut app) = (false, Entry::default(), true, false);
    let mut names = vec![];
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            section = line == "[Desktop Entry]";
            continue;
        }
        let Some((key, value)) = line.split_once('=').filter(|_| section) else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "Name" => entry.name = value.into(),
            "Icon" => entry.icon = value.into(),
            "Type" => app = value == "Application",
            "Hidden" if value == "true" => return None,
            "NoDisplay" => visible = value != "true",
            "Exec" | "TryExec" => names.extend(exec_name(value)),
            "StartupWMClass" => names.push(value.to_lowercase()),
            _ => {}
        }
    }
    (app && !entry.name.is_empty()).then_some((entry, visible, names))
}
fn collect(dir: &Path, prefix: &str, out: &mut Vec<(String, PathBuf)>) {
    for e in fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let path = e.path();
        if path.is_dir() {
            collect(&path, &format!("{prefix}{name}-"), out);
        } else if let Some(id) = name.strip_suffix(".desktop") {
            out.push((format!("{prefix}{id}"), path));
        }
    }
}
/// Desktop-ID candidates from an XDG app cgroup leaf, e.g. `app-gnome-brave\x2dbrowser-123.scope`,
/// `app-flatpak-com.spotify.Client-9.scope`, `dbus-:1.2-org.gnome.Nautilus@0.service`, or
/// `snap.spotify.spotify-<uuid>.scope`.
fn scope_ids(cgroup: &str) -> Vec<String> {
    let leaf = basename(
        cgroup
            .lines()
            .find_map(|l| l.strip_prefix("0::"))
            .unwrap_or(cgroup),
    );
    let unit = leaf
        .strip_suffix(".scope")
        .or_else(|| leaf.strip_suffix(".service"))
        .unwrap_or("");
    let unit = unit.split('@').next().unwrap_or(unit);
    if let Some(snap) = unit.strip_prefix("snap.") {
        let mut parts = snap.split('-').next().unwrap_or(snap).splitn(2, '.');
        return match (parts.next(), parts.next()) {
            (Some(name), Some(app)) => vec![format!("{name}_{app}"), name.into()],
            (Some(name), None) => vec![name.into()],
            _ => vec![],
        };
    }
    if !unit.starts_with("app-") && !unit.starts_with("dbus-") {
        return vec![];
    }
    unit.split('-')
        .skip(1)
        .map(|c| c.replace("\\x2d", "-").to_lowercase())
        .collect()
}
fn find_icon(name: &str, data: &[PathBuf]) -> Option<PathBuf> {
    let path = Path::new(name);
    if path.is_absolute() {
        return path.is_file().then(|| path.into());
    }
    let home = env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let themes = data
        .iter()
        .map(|d| d.join("icons/hicolor"))
        .chain([home.join(".icons/hicolor")]);
    for theme in themes.filter(|t| t.is_dir()).collect::<Vec<_>>() {
        for size in SIZES {
            let ext = if size == "scalable" { "svg" } else { "png" };
            let file = theme.join(format!("{size}/apps/{name}.{ext}"));
            if file.is_file() {
                return Some(file);
            }
        }
    }
    let stem = name.trim_end_matches(".png").trim_end_matches(".svg");
    data.iter()
        .flat_map(|d| ["png", "svg"].map(|ext| d.join(format!("pixmaps/{stem}.{ext}"))))
        .find(|f| f.is_file())
}
impl Apps {
    pub fn new() -> Self {
        let mut apps = Self::empty(data_dirs());
        apps.refresh();
        apps
    }
    fn empty(data: Vec<PathBuf>) -> Self {
        Self {
            data,
            stamps: vec![],
            entries: HashMap::new(),
            aliases: HashMap::new(),
            weak: HashMap::new(),
            resolved: HashMap::new(),
        }
    }
    // Directory mtimes change when desktop files are installed or removed.
    fn refresh(&mut self) {
        let stamps: Vec<_> = self
            .data
            .iter()
            .map(|d| {
                fs::metadata(d.join("applications"))
                    .and_then(|m| m.modified())
                    .ok()
            })
            .collect();
        if stamps == self.stamps {
            return;
        }
        let mut next = Self::empty(self.data.clone());
        next.stamps = stamps;
        for dir in &self.data {
            let mut files = vec![];
            collect(&dir.join("applications"), "", &mut files);
            for (id, path) in files {
                if let Some((entry, visible, names)) = fs::read_to_string(path)
                    .ok()
                    .as_deref()
                    .and_then(parse_entry)
                {
                    next.insert(id, entry, visible, names);
                }
            }
        }
        *self = next;
    }
    fn insert(&mut self, id: String, entry: Entry, visible: bool, names: Vec<String>) {
        let id = id.to_lowercase();
        if self.entries.contains_key(&id) {
            return; // Earlier data directories take precedence.
        }
        if visible {
            let mut exact = names;
            exact.push(id.clone());
            if !entry.icon.contains('/') {
                exact.push(entry.icon.to_lowercase());
            }
            if let Some((_, last)) = id.rsplit_once('.') {
                exact.push(last.into());
            }
            for name in &exact {
                if let Some((first, _)) = name.split_once('-').filter(|(f, _)| f.len() >= 4) {
                    self.weak.entry(first.into()).or_insert(id.clone());
                }
            }
            for name in exact.into_iter().filter(|n| !n.is_empty()) {
                self.aliases.entry(name).or_insert(id.clone());
            }
        }
        self.entries.insert(id, entry);
    }
    fn app(&mut self, id: &str) -> Option<Arc<App>> {
        if let Some(app) = self.resolved.get(id) {
            return app.clone();
        }
        let app = self.entries.get(id).map(|e| {
            Arc::new(App {
                name: e.name.clone(),
                icon: (!e.icon.is_empty())
                    .then(|| find_icon(&e.icon, &self.data))
                    .flatten(),
            })
        });
        self.resolved.insert(id.into(), app.clone());
        app
    }
    fn by_name(&self, p: &Process) -> Option<String> {
        let names = [
            p.name.to_lowercase(),
            basename(p.command.split(' ').next().unwrap_or("")).to_lowercase(),
        ];
        [&self.aliases, &self.weak]
            .iter()
            .find_map(|map| {
                names
                    .iter()
                    .filter(|n| !n.is_empty())
                    .find_map(|n| map.get(n))
            })
            .cloned()
    }
    /// Assigns `group` and `app`. A process belongs to the desktop app that owns its cgroup
    /// scope. Otherwise, within its parent's cgroup, it joins a parent of the same name, then
    /// its own desktop match, then a parent that is a known app.
    pub fn group(&mut self, processes: &mut [Process], cgroups: &[String]) {
        self.refresh();
        let index: HashMap<u32, usize> = processes
            .iter()
            .enumerate()
            .map(|(i, p)| (p.pid, i))
            .collect();
        let mut memo = vec![None; processes.len()];
        for i in 0..processes.len() {
            self.resolve(i, 0, processes, cgroups, &index, &mut memo);
        }
        for (p, (group, app)) in processes.iter_mut().zip(memo.into_iter().flatten()) {
            p.group = group;
            p.app = app;
        }
    }
    fn resolve(
        &mut self,
        i: usize,
        depth: usize,
        processes: &[Process],
        cgroups: &[String],
        index: &HashMap<u32, usize>,
        memo: &mut [Option<Membership>],
    ) -> Membership {
        if let Some(done) = &memo[i] {
            return done.clone();
        }
        let p = &processes[i];
        let scoped = scope_ids(&cgroups[i])
            .into_iter()
            .find_map(|id| Some((self.app(&id)?, id)));
        let parent = index
            .get(&p.ppid)
            .copied()
            .filter(|&j| j != i && depth < 64 && cgroups[j] == cgroups[i])
            .map(|j| {
                (
                    j,
                    self.resolve(j, depth + 1, processes, cgroups, index, memo),
                )
            });
        let result = if let Some((app, id)) = scoped {
            (format!("app:{id}"), Some(app))
        } else if let Some((_, group)) =
            parent.clone().filter(|(j, _)| processes[*j].name == p.name)
        {
            group
        } else if let Some((app, id)) = self.by_name(p).and_then(|id| Some((self.app(&id)?, id))) {
            (format!("app:{id}"), Some(app))
        } else if let Some((_, group)) = parent.filter(|(_, (_, app))| app.is_some()) {
            group
        } else {
            (format!("pid:{}", p.pid), None)
        };
        memo[i] = Some(result.clone());
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn apps() -> Apps {
        let mut apps = Apps::empty(vec![]);
        for (id, text) in [
            (
                "brave-browser",
                "[Desktop Entry]\nName=Brave Web Browser\nExec=/usr/bin/brave-browser-stable %U\nType=Application\nIcon=brave-browser\nStartupWMClass=brave-browser\n[Desktop Action new]\nName=New Window",
            ),
            (
                "discord",
                "[Desktop Entry]\nName=Discord\nExec=/usr/bin/discord\nType=Application",
            ),
            (
                "org.gnome.Shell",
                "[Desktop Entry]\nName=GNOME Shell\nExec=gnome-shell\nType=Application",
            ),
        ] {
            let (entry, visible, names) = parse_entry(text).unwrap();
            apps.insert(id.into(), entry, visible, names);
        }
        apps
    }
    fn process(pid: u32, ppid: u32, name: &str) -> Process {
        Process {
            pid,
            ppid,
            name: name.into(),
            ..Default::default()
        }
    }
    #[test]
    fn desktop_entries_ignore_actions_hidden_and_wrappers() {
        let (entry, visible, names) = parse_entry(
            "[Desktop Entry]\nName=Steam\nExec=env GDK_SCALE=1 /usr/games/steam %U\nType=Application\n[Desktop Action x]\nName=Install",
        )
        .unwrap();
        assert_eq!(entry.name, "Steam");
        assert!(visible);
        assert_eq!(names, ["steam"]);
        assert!(parse_entry("[Desktop Entry]\nName=X\nType=Application\nHidden=true").is_none());
        assert_eq!(exec_name("sh -c \"foo\""), None);
    }
    #[test]
    fn scope_names_yield_desktop_ids() {
        let ids = |s| scope_ids(s);
        assert!(
            ids("0::/user.slice/app.slice/app-gnome-brave\\x2dbrowser-42.scope")
                .contains(&"brave-browser".into())
        );
        assert!(
            ids("/app-flatpak-com.spotify.Client-9.scope").contains(&"com.spotify.client".into())
        );
        assert!(
            ids("/dbus-:1.2-org.gnome.Nautilus@0.service").contains(&"org.gnome.nautilus".into())
        );
        assert_eq!(
            ids("/snap.spotify.spotify-1a2b.scope")[0],
            "spotify_spotify"
        );
        assert!(ids("/session.slice/org.gnome.Shell@wayland.service").is_empty());
    }
    #[test]
    fn processes_group_by_scope_then_tree_then_name() {
        let mut apps = apps();
        let shell = "/session.slice/org.gnome.Shell@wayland.service".to_string();
        let discord = "/app.slice/app-gnome-discord-7.scope".to_string();
        let term = "/app.slice/vte-spawn-1.scope".to_string();
        let mut ps = vec![
            process(10, 1, "gnome-shell"),
            process(11, 10, "brave"),
            process(12, 11, "brave"),
            process(13, 11, "chrome_crashpad"),
            process(20, 1, "Discord"),
            process(21, 20, "Discord"),
            process(30, 1, "bash"),
            process(31, 30, "cargo"),
        ];
        let cgroups = [
            &shell, &shell, &shell, &shell, &discord, &discord, &term, &term,
        ]
        .map(|s| s.clone());
        apps.group(&mut ps, &cgroups);
        let g: Vec<_> = ps.iter().map(|p| p.group.as_str()).collect();
        // GNOME Shell's service unit is not an app scope. Brave matches the first part of its
        // desktop ID instead of joining its GNOME Shell parent; the crash handler follows Brave.
        assert_eq!(g[0], "app:org.gnome.shell");
        assert_eq!(&g[1..4], ["app:brave-browser"; 3]);
        assert_eq!(&g[4..6], ["app:discord"; 2]);
        assert_eq!(&g[6..], ["pid:30", "pid:31"]);
        assert_eq!(ps[1].app.as_ref().unwrap().name, "Brave Web Browser");
    }
}
