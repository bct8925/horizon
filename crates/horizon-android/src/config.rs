//! `horizon-android.txt`, next to the pushed resources on shared storage: what the
//! launcher would pass on a desktop, written by hand (or by `adb push`).
//!
//! ```text
//! # The offline player name (singleplayer and LAN).
//! name=Steve
//! # More game arguments, split on spaces.
//! args=--render-distance 8
//! # An environment variable for the game, such as the graphics backend.
//! env.WGPU_BACKEND=gl
//! ```
use std::path::Path;

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub(crate) const FILE: &str = "horizon-android.txt";

#[derive(Debug, Default, PartialEq)]
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub(crate) struct Config {
    pub(crate) name: Option<String>,
    pub(crate) args: Vec<String>,
    pub(crate) env: Vec<(String, String)>,
}

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
impl Config {
    /// The file's settings; none when it is missing or unreadable.
    pub(crate) fn read(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text),
            Err(_) => Self::default(),
        }
    }

    pub(crate) fn parse(text: &str) -> Self {
        let mut config = Self::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                eprintln!("Horizon: {FILE}: ignoring \"{line}\" (no '=')");
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            match key {
                "name" if !value.is_empty() => config.name = Some(value.to_owned()),
                "args" => config
                    .args
                    .extend(value.split_whitespace().map(str::to_owned)),
                _ => match key.strip_prefix("env.") {
                    Some(var) if !var.is_empty() => {
                        config.env.push((var.to_owned(), value.to_owned()))
                    }
                    _ => eprintln!("Horizon: {FILE}: unknown setting {key}"),
                },
            }
        }
        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_name_args_and_env() {
        let config = Config::parse(
            "# comment\nname = Alex\nargs=--render-distance 8  --no-pause\nenv.WGPU_BACKEND=gl\nbogus\n",
        );
        assert_eq!(config.name.as_deref(), Some("Alex"));
        assert_eq!(config.args, ["--render-distance", "8", "--no-pause"]);
        assert_eq!(config.env, [("WGPU_BACKEND".into(), "gl".into())]);
    }
}
