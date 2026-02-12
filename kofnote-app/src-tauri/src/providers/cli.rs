use crate::types::CODEX_MODEL_FALLBACKS;
use serde_json::Value;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration as StdDuration, Instant, SystemTime, UNIX_EPOCH};

pub(crate) struct CliInvocation {
    pub(crate) args: Vec<String>,
    pub(crate) stdin_payload: Option<String>,
    pub(crate) output_file: Option<PathBuf>,
}

pub(crate) struct CliProviderConfig {
    pub(crate) id: &'static str,
    pub(crate) command: &'static str,
    pub(crate) build_args:
        fn(model: Option<&str>, prompt: &str, timeout_secs: u64, max_tokens: u32) -> CliInvocation,
    pub(crate) parse_output:
        fn(status: &ExitStatus, stdout: &str, stderr: &str, output_text: &str) -> Option<String>,
    pub(crate) failure_hint: fn(stdout: &str, stderr: &str) -> String,
    pub(crate) model_fallbacks: &'static [&'static str],
}

pub(crate) fn run_cli_command_with_timeout(
    command: &str,
    args: &[String],
    stdin_payload: Option<&str>,
    timeout_secs: u64,
) -> Result<(ExitStatus, String, String), String> {
    let mut child = Command::new(command)
        .args(args)
        .stdin(if stdin_payload.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Failed to start `{command}`: {error}"))?;

    if let Some(payload) = stdin_payload {
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(payload.as_bytes())
                .map_err(|error| format!("Failed to write stdin to `{command}`: {error}"))?;
        }
    }

    let stdout_handle = child.stdout.take().map(|mut stdout| {
        thread::spawn(move || {
            let mut text = String::new();
            let _ = stdout.read_to_string(&mut text);
            text
        })
    });
    let stderr_handle = child.stderr.take().map(|mut stderr| {
        thread::spawn(move || {
            let mut text = String::new();
            let _ = stderr.read_to_string(&mut text);
            text
        })
    });

    let deadline = Instant::now() + StdDuration::from_secs(timeout_secs.max(5));
    let status = loop {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("`{command}` timed out after {timeout_secs}s"));
        }

        match child.try_wait() {
            Ok(Some(done)) => break done,
            Ok(None) => thread::sleep(StdDuration::from_millis(120)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("Failed while waiting for `{command}`: {error}"));
            }
        }
    };

    let stdout_text = match stdout_handle {
        Some(handle) => handle.join().unwrap_or_default(),
        None => String::new(),
    };
    let stderr_text = match stderr_handle {
        Some(handle) => handle.join().unwrap_or_default(),
        None => String::new(),
    };

    Ok((status, stdout_text, stderr_text))
}

pub(crate) fn summarize_cli_stream(stream: &str) -> String {
    let lines = stream
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();

    if lines.is_empty() {
        return String::new();
    }

    if lines.len() <= 8 {
        return lines.join(" | ");
    }

    let mut summary = Vec::new();
    summary.extend(lines.iter().take(4).cloned());
    summary.push(format!("... ({} lines omitted) ...", lines.len() - 8));
    summary.extend(
        lines
            .iter()
            .rev()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev(),
    );
    summary.join(" | ")
}

fn extract_cli_json_text(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        let clean = text.trim();
        if !clean.is_empty() {
            return Some(clean.to_string());
        }
    }

    for key in ["result", "response", "output", "answer", "text", "message"] {
        if let Some(text) = value.get(key).and_then(Value::as_str) {
            let clean = text.trim();
            if !clean.is_empty() {
                return Some(clean.to_string());
            }
        }
    }

    if let Some(content) = value.get("content").and_then(Value::as_array) {
        let joined = content
            .iter()
            .filter_map(|item| {
                item.get("text")
                    .and_then(Value::as_str)
                    .or_else(|| item.as_str())
                    .map(str::to_string)
            })
            .collect::<Vec<_>>()
            .join("\n");
        if !joined.trim().is_empty() {
            return Some(joined.trim().to_string());
        }
    }

    None
}

pub(crate) fn parse_cli_output_text(stdout: &str) -> Option<String> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        if let Some(text) = extract_cli_json_text(&value) {
            return Some(text);
        }
    }

    Some(trimmed.to_string())
}

pub(crate) fn normalize_cli_model_arg(provider_id: &str, model: &str) -> Option<String> {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return None;
    }

    let normalized = trimmed.to_lowercase();
    if matches!(normalized.as_str(), "auto" | "default") {
        return None;
    }
    if provider_id == "codex-cli" && normalized == "codex" {
        return None;
    }
    if provider_id == "gemini-cli" && normalized == "gemini" {
        return None;
    }
    if provider_id == "claude-cli" && normalized == "claude" {
        return None;
    }

    Some(trimmed.to_string())
}

pub(crate) fn is_cli_model_error(stdout: &str, stderr: &str) -> bool {
    let combined = format!("{}\n{}", stdout.to_lowercase(), stderr.to_lowercase());
    combined.contains("invalid model")
        || combined.contains("unknown model")
        || combined.contains("unsupported model")
        || combined.contains("not a supported model")
        || combined.contains("inaccessible model")
        || (combined.contains("model") && combined.contains("does not exist"))
        || (combined.contains("model") && combined.contains("do not have access"))
        || (combined.contains("model") && combined.contains("not available"))
        || (combined.contains("model") && combined.contains("not found"))
}

fn read_and_cleanup_output_file(path: &Path) -> String {
    let text = fs::read_to_string(path).unwrap_or_default();
    let _ = fs::remove_file(path);
    text
}

pub(crate) fn build_codex_cli_args(
    model: Option<&str>,
    _prompt: &str,
    _timeout_secs: u64,
    _max_tokens: u32,
) -> CliInvocation {
    let output_path = std::env::temp_dir().join(format!(
        "kofnote_codex_debate_{}_{}.txt",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| StdDuration::from_secs(0))
            .as_millis()
    ));

    let mut args = vec![
        "exec".to_string(),
        "-".to_string(),
        "-c".to_string(),
        "model_reasoning_effort=\"high\"".to_string(),
        "--skip-git-repo-check".to_string(),
        "--sandbox".to_string(),
        "read-only".to_string(),
        "--output-last-message".to_string(),
        output_path.to_string_lossy().to_string(),
        "--color".to_string(),
        "never".to_string(),
    ];
    if let Some(model_name) = model {
        args.push("--model".to_string());
        args.push(model_name.to_string());
    }

    CliInvocation {
        args,
        stdin_payload: Some(String::new()),
        output_file: Some(output_path),
    }
}

pub(crate) fn build_gemini_cli_args(
    model: Option<&str>,
    prompt: &str,
    _timeout_secs: u64,
    _max_tokens: u32,
) -> CliInvocation {
    let mut args = vec![
        prompt.to_string(),
        "--output-format".to_string(),
        "json".to_string(),
    ];
    if let Some(model_name) = model {
        args.push("--model".to_string());
        args.push(model_name.to_string());
    }
    CliInvocation {
        args,
        stdin_payload: None,
        output_file: None,
    }
}

pub(crate) fn build_claude_cli_args(
    model: Option<&str>,
    prompt: &str,
    _timeout_secs: u64,
    _max_tokens: u32,
) -> CliInvocation {
    let mut args = vec![
        "--print".to_string(),
        "--output-format".to_string(),
        "json".to_string(),
    ];
    if let Some(model_name) = model {
        args.push("--model".to_string());
        args.push(model_name.to_string());
    }
    args.push(prompt.to_string());
    CliInvocation {
        args,
        stdin_payload: None,
        output_file: None,
    }
}

fn parse_codex_cli_output(
    _status: &ExitStatus,
    stdout_text: &str,
    _stderr_text: &str,
    output_text: &str,
) -> Option<String> {
    if !output_text.trim().is_empty() {
        return parse_cli_output_text(output_text);
    }
    parse_cli_output_text(stdout_text)
}

fn parse_json_stdout_output(
    _status: &ExitStatus,
    _stdout_text: &str,
    _stderr_text: &str,
    output_text: &str,
) -> Option<String> {
    parse_cli_output_text(output_text)
}

const CODEX_CLI_CONFIG: CliProviderConfig = CliProviderConfig {
    id: "codex-cli",
    command: "codex",
    build_args: build_codex_cli_args,
    parse_output: parse_codex_cli_output,
    failure_hint: codex_cli_failure_hint,
    model_fallbacks: &CODEX_MODEL_FALLBACKS,
};

const GEMINI_CLI_CONFIG: CliProviderConfig = CliProviderConfig {
    id: "gemini-cli",
    command: "gemini",
    build_args: build_gemini_cli_args,
    parse_output: parse_json_stdout_output,
    failure_hint: gemini_cli_failure_hint,
    model_fallbacks: &[],
};

const CLAUDE_CLI_CONFIG: CliProviderConfig = CliProviderConfig {
    id: "claude-cli",
    command: "claude",
    build_args: build_claude_cli_args,
    parse_output: parse_json_stdout_output,
    failure_hint: claude_cli_failure_hint,
    model_fallbacks: &[],
};

fn run_cli_provider_once(
    config: &CliProviderConfig,
    model_override: Option<&str>,
    prompt: &str,
    timeout_secs: u64,
    max_turn_tokens: u32,
) -> Result<(ExitStatus, String, String, String), String> {
    let mut invocation = (config.build_args)(model_override, prompt, timeout_secs, max_turn_tokens);

    if config.id == "codex-cli" && invocation.stdin_payload.is_some() {
        invocation.stdin_payload = Some(prompt.to_string());
    }

    let output_file = invocation.output_file.clone();
    let result = run_cli_command_with_timeout(
        config.command,
        &invocation.args,
        invocation.stdin_payload.as_deref(),
        timeout_secs,
    );

    match result {
        Ok((status, stdout_text, stderr_text)) => {
            let output_text = output_file
                .as_deref()
                .map(read_and_cleanup_output_file)
                .unwrap_or_else(|| stdout_text.clone());
            Ok((status, stdout_text, stderr_text, output_text))
        }
        Err(error) => {
            if let Some(path) = output_file.as_deref() {
                let _ = fs::remove_file(path);
            }
            Err(error)
        }
    }
}

pub(crate) fn run_cli_provider(
    config: &CliProviderConfig,
    model: &str,
    prompt: &str,
    max_turn_seconds: u64,
    max_turn_tokens: u32,
) -> Result<String, String> {
    let timeout_secs = max_turn_seconds.clamp(10, 180);
    let model_override = normalize_cli_model_arg(config.id, model);
    let mut attempts = Vec::new();

    let (first_status, mut last_stdout, mut last_stderr, first_output) = run_cli_provider_once(
        config,
        model_override.as_deref(),
        prompt,
        timeout_secs,
        max_turn_tokens,
    )?;
    if first_status.success() {
        if let Some(text) = (config.parse_output)(&first_status, &last_stdout, &last_stderr, &first_output)
        {
            return Ok(text);
        }
        let hint = (config.failure_hint)(&last_stdout, &last_stderr);
        return Err(format!(
            "`{}` returned empty output. {hint} stderr={}",
            config.command,
            summarize_cli_stream(&last_stderr)
        ));
    }
    attempts.push(format!(
        "{}: status={} stderr={}",
        model_override
            .as_deref()
            .map(|item| format!("model:{item}"))
            .unwrap_or_else(|| "model:auto".to_string()),
        first_status,
        summarize_cli_stream(&last_stderr)
    ));

    let mut retry_models: Vec<Option<String>> = Vec::new();
    if is_cli_model_error(&last_stdout, &last_stderr) {
        if model_override.is_some() {
            retry_models.push(None);
        }
        for fallback in config.model_fallbacks {
            if Some(*fallback) == model_override.as_deref() {
                continue;
            }
            retry_models.push(Some((*fallback).to_string()));
        }
    }

    for retry_model in retry_models {
        let (retry_status, retry_stdout, retry_stderr, retry_output) =
            run_cli_provider_once(config, retry_model.as_deref(), prompt, timeout_secs, max_turn_tokens)?;
        if retry_status.success() {
            if let Some(text) =
                (config.parse_output)(&retry_status, &retry_stdout, &retry_stderr, &retry_output)
            {
                return Ok(text);
            }
            return Err(format!(
                "`{}` retry succeeded but returned empty output. model={} stderr={}",
                config.command,
                retry_model.as_deref().unwrap_or("auto"),
                summarize_cli_stream(&retry_stderr)
            ));
        }

        attempts.push(format!(
            "{}: status={} stderr={}",
            retry_model
                .as_deref()
                .map(|item| format!("model:{item}"))
                .unwrap_or_else(|| "model:auto".to_string()),
            retry_status,
            summarize_cli_stream(&retry_stderr)
        ));
        last_stdout = retry_stdout;
        last_stderr = retry_stderr;
        if !is_cli_model_error(&last_stdout, &last_stderr) {
            break;
        }
    }

    let hint = (config.failure_hint)(&last_stdout, &last_stderr);
    Err(format!(
        "`{}` failed after retries. {hint} attempts={}",
        config.command,
        attempts.join(" || ")
    ))
}

pub(crate) fn run_codex_cli_completion(
    model: &str,
    prompt: &str,
    max_turn_seconds: u64,
    max_turn_tokens: u32,
) -> Result<String, String> {
    run_cli_provider(
        &CODEX_CLI_CONFIG,
        model,
        prompt,
        max_turn_seconds,
        max_turn_tokens,
    )
}

pub(crate) fn run_gemini_cli_completion(
    model: &str,
    prompt: &str,
    max_turn_seconds: u64,
    max_turn_tokens: u32,
) -> Result<String, String> {
    run_cli_provider(
        &GEMINI_CLI_CONFIG,
        model,
        prompt,
        max_turn_seconds,
        max_turn_tokens,
    )
}

pub(crate) fn run_claude_cli_completion(
    model: &str,
    prompt: &str,
    max_turn_seconds: u64,
    max_turn_tokens: u32,
) -> Result<String, String> {
    run_cli_provider(
        &CLAUDE_CLI_CONFIG,
        model,
        prompt,
        max_turn_seconds,
        max_turn_tokens,
    )
}

pub(crate) fn codex_cli_failure_hint(stdout: &str, stderr: &str) -> String {
    let combined = format!("{}\n{}", stdout.to_lowercase(), stderr.to_lowercase());
    if is_cli_model_error(stdout, stderr) {
        return "Model is not available for codex-cli. Leave model blank (auto) or choose a codex-supported model.".to_string();
    }
    if combined.contains("cannot access session files")
        || (combined.contains(".codex/sessions") && combined.contains("permission denied"))
    {
        return "Codex session directory permission denied. Fix with: sudo chown -R $(whoami) ~/.codex".to_string();
    }
    if combined.contains("login") && combined.contains("codex") {
        return "Codex may not be authenticated. Run `codex login` in terminal first.".to_string();
    }
    if combined.contains("error sending request for url")
        || combined.contains("stream disconnected")
        || combined.contains("network error")
    {
        return "Codex network/API call failed. Check network and model access.".to_string();
    }
    "Check `codex exec` manually in terminal to inspect full error output.".to_string()
}

pub(crate) fn gemini_cli_failure_hint(stdout: &str, stderr: &str) -> String {
    let combined = format!("{}\n{}", stdout.to_lowercase(), stderr.to_lowercase());
    if is_cli_model_error(stdout, stderr) {
        return "Model is not available for gemini-cli. Leave model blank (auto) or choose a Gemini CLI supported model.".to_string();
    }
    if combined.contains("login") || combined.contains("auth") {
        return "Gemini CLI may not be authenticated. Run `gemini` once to complete login/auth.".to_string();
    }
    if combined.contains("api key") {
        return "Gemini CLI requires API key/auth setup. Check your Gemini CLI auth configuration.".to_string();
    }
    if combined.contains("network error") || combined.contains("connection") {
        return "Gemini CLI network/API call failed. Check network and CLI status.".to_string();
    }
    "Check `gemini` command manually in terminal to inspect full error output.".to_string()
}

pub(crate) fn claude_cli_failure_hint(stdout: &str, stderr: &str) -> String {
    let combined = format!("{}\n{}", stdout.to_lowercase(), stderr.to_lowercase());
    if is_cli_model_error(stdout, stderr) {
        return "Model is not available for claude-cli. Leave model blank (auto) or choose a Claude CLI supported model.".to_string();
    }
    if combined.contains("login") || combined.contains("auth") {
        return "Claude CLI may not be authenticated. Run `claude` once to complete login/auth.".to_string();
    }
    if combined.contains("api key") {
        return "Claude CLI requires API key/auth setup. Check your Claude CLI auth configuration.".to_string();
    }
    if combined.contains("network error") || combined.contains("connection") {
        return "Claude CLI network/API call failed. Check network and CLI status.".to_string();
    }
    "Check `claude` command manually in terminal to inspect full error output.".to_string()
}
