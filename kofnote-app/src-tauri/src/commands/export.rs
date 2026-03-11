use crate::commands::core::compute_dashboard_stats;
use crate::storage::records::{load_logs, load_records, normalized_home};
use crate::types::{DashboardStats, ExportReportResult, Record};
use crate::util::{absolute_path, extract_day, write_atomic};
use chrono::{Duration as ChronoDuration, Local, NaiveDate};
use std::fs;
use std::path::Path;

fn render_report_markdown(
    title: &str,
    central_home: &Path,
    stats: &DashboardStats,
    recent_records: &[&Record],
    days: i64,
) -> String {
    let mut lines = vec![
        format!("# {}", title),
        String::new(),
        format!("Generated: {}", Local::now().to_rfc3339()),
        format!("Central Home: {}", central_home.to_string_lossy()),
        String::new(),
        "## KPI".to_string(),
        format!("- Total records: {}", stats.total_records),
        format!("- Total logs: {}", stats.total_logs),
        format!("- Pending sync: {}", stats.pending_sync_count),
        String::new(),
        "## Type Distribution".to_string(),
    ];
    for (record_type, count) in &stats.type_counts {
        lines.push(format!("- {}: {}", record_type, count));
    }
    lines.push(String::new());
    lines.push("## Top Tags".to_string());
    if stats.top_tags.is_empty() {
        lines.push("- (none)".to_string());
    } else {
        for item in &stats.top_tags {
            lines.push(format!("- {} ({})", item.tag, item.count));
        }
    }
    lines.push(String::new());
    lines.push(format!("## Recent Records (last {} days)", days));
    for item in recent_records {
        lines.push(format!(
            "- [{}] ({}) {}",
            item.created_at, item.record_type, item.title
        ));
    }
    lines.join("\n")
}

#[tauri::command]
pub fn export_markdown_report(
    central_home: String,
    output_path: Option<String>,
    title: Option<String>,
    recent_days: Option<i64>,
) -> Result<ExportReportResult, String> {
    let home = normalized_home(&central_home)?;
    let records = load_records(&home)?;
    let logs = load_logs(&home)?;
    let stats = compute_dashboard_stats(&records, &logs);

    let now = Local::now();
    let title = title.unwrap_or_else(|| format!("KOF Report {}", now.format("%Y-%m-%d")));
    let days = recent_days.unwrap_or(7).clamp(1, 365);
    let cutoff = now.date_naive() - ChronoDuration::days(days);

    let recent_records: Vec<&Record> = records
        .iter()
        .filter(|item| {
            extract_day(&item.created_at)
                .and_then(|day| NaiveDate::parse_from_str(&day, "%Y-%m-%d").ok())
                .map(|date| date >= cutoff)
                .unwrap_or(false)
        })
        .take(80)
        .collect();

    let report_md = render_report_markdown(&title, &home, &stats, &recent_records, days);

    let target = if let Some(path) = output_path {
        absolute_path(Path::new(path.trim()))
    } else {
        let report_dir = home.join("assets").join("reports");
        fs::create_dir_all(&report_dir).map_err(|e| e.to_string())?;
        report_dir.join(format!("{}_kof-report.md", now.format("%Y%m%d_%H%M%S")))
    };

    write_atomic(&target, report_md.as_bytes()).map_err(|e| e.to_string())?;

    Ok(ExportReportResult {
        output_path: target.to_string_lossy().to_string(),
        title,
    })
}
