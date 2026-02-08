from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


RECORD_SUBDIR_BY_TYPE = {
    "decision": "decisions",
    "worklog": "worklogs",
    "idea": "ideas",
    "backlog": "backlogs",
    "note": "other",
}

TYPE_BY_RECORD_SUBDIR = {value: key for key, value in RECORD_SUBDIR_BY_TYPE.items()}


@dataclass
class Record:
    record_type: str
    title: str
    created_at: str
    source_text: str
    final_body: str
    tags: list[str] = field(default_factory=list)
    date: str | None = None
    notion_page_id: str | None = None
    notion_url: str | None = None
    notion_sync_status: str = "SUCCESS"
    notion_error: str | None = None
    json_path: Path | None = None
    md_path: Path | None = None

    @classmethod
    def from_storage_dict(
        cls,
        data: dict[str, Any],
        json_path: Path | None = None,
        md_path: Path | None = None,
    ) -> "Record":
        tags = data.get("tags", [])
        if not isinstance(tags, list):
            tags = []

        return cls(
            record_type=str(data.get("type", "note") or "note").lower(),
            title=str(data.get("title", "Untitled")),
            created_at=str(data.get("created_at", "")),
            source_text=str(data.get("source_text", "")),
            final_body=str(data.get("final_body", "")),
            tags=[str(tag) for tag in tags],
            date=(str(data["date"]) if data.get("date") else None),
            notion_page_id=(
                str(data["notion_page_id"])
                if data.get("notion_page_id")
                else None
            ),
            notion_url=(str(data["notion_url"]) if data.get("notion_url") else None),
            notion_sync_status=str(data.get("notion_sync_status", "SUCCESS")),
            notion_error=(str(data["notion_error"]) if data.get("notion_error") else None),
            json_path=json_path,
            md_path=md_path,
        )

    def to_storage_dict(self) -> dict[str, Any]:
        return {
            "type": self.record_type,
            "title": self.title,
            "created_at": self.created_at,
            "notion_page_id": self.notion_page_id,
            "notion_url": self.notion_url,
            "source_text": self.source_text,
            "final_body": self.final_body,
            "tags": self.tags,
            "date": self.date,
            "notion_sync_status": self.notion_sync_status,
            "notion_error": self.notion_error,
        }


@dataclass
class CentralLogEntry:
    timestamp: str
    event_id: str
    task_intent: str
    status: str
    title: str
    data: dict[str, Any]
    raw: dict[str, Any]
    json_path: Path | None = None

    @classmethod
    def from_storage_dict(
        cls,
        data: dict[str, Any],
        json_path: Path | None = None,
    ) -> "CentralLogEntry":
        meta = data.get("meta", {}) if isinstance(data.get("meta"), dict) else {}
        task = data.get("task", {}) if isinstance(data.get("task"), dict) else {}
        payload = data.get("data", {}) if isinstance(data.get("data"), dict) else {}

        title = str(payload.get("title", ""))
        return cls(
            timestamp=str(meta.get("timestamp", "")),
            event_id=str(meta.get("event_id", "")),
            task_intent=str(task.get("intent", "")),
            status=str(task.get("status", "")),
            title=title,
            data=payload,
            raw=data,
            json_path=json_path,
        )


def normalized_record_type(record_type: str) -> str:
    record_type = (record_type or "note").strip().lower()
    return record_type if record_type in RECORD_SUBDIR_BY_TYPE else "note"
