//! Startup CLI: `separate --model <id> --path <file>`

use std::path::PathBuf;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliSeparateRequest {
    pub model: String,
    pub path: String,
    pub output_dir: Option<String>,
    pub flatten_output: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupCli {
    pub separate: Option<CliSeparateRequest>,
    pub analyze: Option<CliAnalyzeRequest>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliAnalyzeRequest {
    pub path: String,
}

pub fn parse_args(args: &[String]) -> StartupCli {
    let mut out = StartupCli::default();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "separate" {
            let mut model: Option<String> = None;
            let mut path = String::new();
            let mut output_dir: Option<String> = None;
            i += 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--model" if i + 1 < args.len() => {
                        model = Some(args[i + 1].clone());
                        i += 2;
                    }
                    "--path" if i + 1 < args.len() => {
                        path = args[i + 1].clone();
                        i += 2;
                    }
                    "--output-dir" if i + 1 < args.len() => {
                        output_dir = Some(args[i + 1].clone());
                        i += 2;
                    }
                    other if !other.starts_with('-') && path.is_empty() => {
                        path = other.to_string();
                        i += 1;
                    }
                    _ => break,
                }
            }
            if let (Some(model), true) = (model, !path.is_empty()) {
                if !matches!(
                    model.as_str(),
                    "melbandroformer_vocals" | "htdemucs_ft" | "htdemucs_6s"
                ) {
                    i += 1;
                    continue;
                }
                let flatten_output = output_dir.is_none();
                if output_dir.is_none() {
                    let parent = PathBuf::from(&path)
                        .parent()
                        .map(|p| p.to_string_lossy().to_string());
                    output_dir = parent;
                }
                out.separate = Some(CliSeparateRequest {
                    model,
                    path,
                    output_dir,
                    flatten_output,
                });
            }
            continue;
        }
        if args[i] == "analyze" {
            let mut path = String::new();
            i += 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--path" if i + 1 < args.len() => {
                        path = args[i + 1].clone();
                        i += 2;
                    }
                    other if !other.starts_with('-') && path.is_empty() => {
                        path = other.to_string();
                        i += 1;
                    }
                    _ => break,
                }
            }
            if !path.is_empty() {
                out.analyze = Some(CliAnalyzeRequest { path });
            }
            continue;
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_separate_command() {
        let args = vec![
            "app".into(),
            "separate".into(),
            "--model".into(),
            "htdemucs_ft".into(),
            "--path".into(),
            r"C:\music\song.wav".into(),
        ];
        let cli = parse_args(&args);
        let sep = cli.separate.expect("separate");
        assert_eq!(sep.model, "htdemucs_ft");
        assert!(sep.path.ends_with("song.wav"));
        assert!(sep.output_dir.unwrap().ends_with("music"));
        assert!(sep.flatten_output);
    }

    #[test]
    fn parses_analyze_command() {
        let args = vec![
            "app".into(),
            "analyze".into(),
            "--path".into(),
            r"C:\music\song.wav".into(),
        ];
        let cli = parse_args(&args);
        assert_eq!(cli.analyze.unwrap().path, r"C:\music\song.wav");
    }

    #[test]
    fn rejects_missing_or_unknown_models() {
        let missing = vec![
            "app".into(),
            "separate".into(),
            "--path".into(),
            "song.wav".into(),
        ];
        assert!(parse_args(&missing).separate.is_none());

        let unknown = vec![
            "app".into(),
            "separate".into(),
            "--model".into(),
            "not-a-model".into(),
            "--path".into(),
            "song.wav".into(),
        ];
        assert!(parse_args(&unknown).separate.is_none());
    }
}
