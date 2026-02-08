from __future__ import annotations

from collections import Counter
from dataclasses import dataclass
from datetime import datetime, timedelta

from .models import CentralLogEntry, Record


@dataclass
class DashboardStats:
    total_records: int
    total_logs: int
    type_counts: dict[str, int]
    top_tags: list[tuple[str, int]]
    recent_daily_counts: dict[str, int]
    pending_sync_count: int


def _iso_date(value: str) -> str | None:
    if not value:
        return None
    try:
        return datetime.fromisoformat(value.replace("Z", "+00:00")).date().isoformat()
    except Exception:
        try:
            return value[:10]
        except Exception:
            return None


def compute_dashboard_stats(records: list[Record], logs: list[CentralLogEntry]) -> DashboardStats:
    type_counter: Counter[str] = Counter()
    tag_counter: Counter[str] = Counter()
    pending_sync = 0

    for record in records:
        type_counter[record.record_type] += 1
        for tag in record.tags:
            clean = tag.strip()
            if clean:
                tag_counter[clean] += 1
        if record.notion_sync_status.upper() in {"PENDING", "FAILED"}:
            pending_sync += 1

    recent_daily_counts: dict[str, int] = {}
    today = datetime.now().date()
    for offset in range(6, -1, -1):
        day = today - timedelta(days=offset)
        recent_daily_counts[day.isoformat()] = 0

    for record in records:
        record_date = _iso_date(record.created_at)
        if record_date in recent_daily_counts:
            recent_daily_counts[record_date] += 1

    for log in logs:
        log_date = _iso_date(log.timestamp)
        if log_date in recent_daily_counts:
            recent_daily_counts[log_date] += 1

    return DashboardStats(
        total_records=len(records),
        total_logs=len(logs),
        type_counts=dict(type_counter),
        top_tags=tag_counter.most_common(10),
        recent_daily_counts=recent_daily_counts,
        pending_sync_count=pending_sync,
    )


def build_context_digest(records: list[Record], logs: list[CentralLogEntry], limit: int = 20) -> str:
    sorted_records = sorted(records, key=lambda item: item.created_at, reverse=True)[:limit]
    sorted_logs = sorted(logs, key=lambda item: item.timestamp, reverse=True)[:limit]

    lines = ["# Records"]
    for item in sorted_records:
        tags = ", ".join(item.tags) if item.tags else "-"
        lines.append(
            f"- [{item.created_at}] ({item.record_type}) {item.title} | tags: {tags}"
        )

    lines.append("\n# Central Logs")
    for item in sorted_logs:
        lines.append(
            f"- [{item.timestamp}] {item.task_intent} / {item.status} / {item.title}"
        )

    return "\n".join(lines)
