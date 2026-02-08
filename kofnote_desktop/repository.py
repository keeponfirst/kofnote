from __future__ import annotations

import json
from datetime import datetime
from pathlib import Path

from .models import (
    RECORD_SUBDIR_BY_TYPE,
    CentralLogEntry,
    Record,
    normalized_record_type,
)


TYPE_EMOJI = {
    "decision": "⚖️",
    "worklog": "📝",
    "idea": "💡",
    "backlog": "📋",
    "note": "📄",
}


class CentralBrainRepository:
    def __init__(self, central_home: Path):
        self.central_home = central_home.resolve()
        self.records_root = self.central_home / "records"
        self.logs_root = self.central_home / ".agentic" / "logs"

    def ensure_structure(self) -> None:
        self.records_root.mkdir(parents=True, exist_ok=True)
        for subdir in RECORD_SUBDIR_BY_TYPE.values():
            (self.records_root / subdir).mkdir(parents=True, exist_ok=True)
        self.logs_root.mkdir(parents=True, exist_ok=True)

    def list_records(self) -> list[Record]:
        records: list[Record] = []
        if not self.records_root.exists():
            return records

        for subdir in RECORD_SUBDIR_BY_TYPE.values():
            folder = self.records_root / subdir
            if not folder.exists():
                continue
            for json_path in sorted(folder.glob("*.json")):
                try:
                    with open(json_path, "r", encoding="utf-8") as f:
                        data = json.load(f)
                    md_path = json_path.with_suffix(".md")
                    record = Record.from_storage_dict(data, json_path=json_path, md_path=md_path)
                    if not record.created_at:
                        record.created_at = self._guess_created_at(json_path)
                    records.append(record)
                except Exception:
                    # Skip malformed files and keep UI responsive.
                    continue

        records.sort(key=lambda record: record.created_at, reverse=True)
        return records

    def list_logs(self) -> list[CentralLogEntry]:
        logs: list[CentralLogEntry] = []
        if not self.logs_root.exists():
            return logs

        for json_path in sorted(self.logs_root.glob("*.json")):
            try:
                with open(json_path, "r", encoding="utf-8") as f:
                    data = json.load(f)
                entry = CentralLogEntry.from_storage_dict(data, json_path=json_path)
                if not entry.timestamp:
                    entry.timestamp = self._guess_created_at(json_path)
                logs.append(entry)
            except Exception:
                continue

        logs.sort(key=lambda entry: entry.timestamp, reverse=True)
        return logs

    def save_record(self, record: Record, existing_json_path: Path | None = None) -> Record:
        self.ensure_structure()

        record.record_type = normalized_record_type(record.record_type)
        if not record.created_at:
            record.created_at = datetime.now().isoformat(timespec="seconds")

        target_subdir = RECORD_SUBDIR_BY_TYPE[record.record_type]
        target_dir = self.records_root / target_subdir
        target_dir.mkdir(parents=True, exist_ok=True)

        if existing_json_path and existing_json_path.exists():
            base_name = existing_json_path.stem
        else:
            base_name = self._generate_filename(record)

        json_path = target_dir / f"{base_name}.json"
        md_path = target_dir / f"{base_name}.md"

        with open(json_path, "w", encoding="utf-8") as f:
            json.dump(record.to_storage_dict(), f, ensure_ascii=False, indent=2)

        with open(md_path, "w", encoding="utf-8") as f:
            f.write(self._record_to_markdown(record))

        if existing_json_path and existing_json_path.exists() and existing_json_path != json_path:
            old_md = existing_json_path.with_suffix(".md")
            existing_json_path.unlink(missing_ok=True)
            old_md.unlink(missing_ok=True)

        saved = Record.from_storage_dict(
            record.to_storage_dict(),
            json_path=json_path,
            md_path=md_path,
        )
        return saved

    def delete_record(self, record: Record) -> None:
        if record.json_path:
            record.json_path.unlink(missing_ok=True)
        if record.md_path:
            record.md_path.unlink(missing_ok=True)

    def _generate_filename(self, record: Record) -> str:
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        slug = self._slugify(record.title)
        return f"{timestamp}_{record.record_type}_{slug}"

    @staticmethod
    def _slugify(text: str, max_length: int = 48) -> str:
        slug = (text or "untitled").lower()
        slug = "".join(ch if ch.isalnum() or ch in "-_" else "-" for ch in slug)
        while "--" in slug:
            slug = slug.replace("--", "-")
        return slug.strip("-")[:max_length] or "untitled"

    @staticmethod
    def _guess_created_at(path: Path) -> str:
        try:
            return datetime.fromtimestamp(path.stat().st_mtime).isoformat(timespec="seconds")
        except Exception:
            return ""

    @staticmethod
    def _record_to_markdown(record: Record) -> str:
        emoji = TYPE_EMOJI.get(record.record_type, "📄")
        lines = [
            f"# {emoji} {record.title}",
            "",
            f"**Type:** {record.record_type.upper()}",
            f"**Created:** {record.created_at}",
        ]

        if record.date:
            lines.append(f"**Date:** {record.date}")
        if record.tags:
            lines.append(f"**Tags:** {', '.join(record.tags)}")
        if record.notion_url:
            lines.append(f"**Notion:** {record.notion_url}")

        lines.extend(
            [
                "",
                "---",
                "",
                record.final_body,
                "",
                "---",
                "",
                "## Original Input",
                "",
                f"> {record.source_text}",
            ]
        )
        return "\n".join(lines)


def detect_central_home(candidate: Path) -> Path:
    path = candidate.expanduser().resolve()
    if path.is_file():
        path = path.parent

    record_subdirs = set(RECORD_SUBDIR_BY_TYPE.values())

    # If user picks records/ or records/<type>, normalize to the central home.
    if path.name in record_subdirs and path.parent.name == "records":
        return path.parent.parent
    if path.name == "records":
        return path.parent

    # If user picks .agentic or .agentic/logs, normalize to the central home.
    if path.name == "logs" and path.parent.name == ".agentic":
        return path.parent.parent
    if path.name == ".agentic":
        return path.parent

    marker = path / ".agentic" / "CENTRAL_LOG_MARKER"
    if marker.exists() or (path / "records").exists() or (path / ".agentic" / "logs").exists():
        return path

    # Ancestor fallback: if a parent looks like central home, use it.
    for parent in path.parents:
        if (parent / ".agentic" / "CENTRAL_LOG_MARKER").exists():
            return parent
        if (parent / "records").exists() and (parent / ".agentic").exists():
            return parent

    return path
