import json
import tempfile
import unittest
from pathlib import Path

from kofnote_desktop.models import Record
from kofnote_desktop.repository import CentralBrainRepository, detect_central_home


class RepositoryTest(unittest.TestCase):
    def test_save_and_list_record(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            repo = CentralBrainRepository(home)
            repo.ensure_structure()

            saved = repo.save_record(
                Record(
                    record_type="idea",
                    title="Desktop 控制台",
                    created_at="2026-02-06T12:30:00",
                    source_text="想做 desktop app",
                    final_body="先做 MVP。",
                    tags=["desktop", "mvp"],
                )
            )

            self.assertIsNotNone(saved.json_path)
            self.assertTrue(saved.json_path.exists())
            self.assertTrue(saved.md_path.exists())

            records = repo.list_records()
            self.assertEqual(len(records), 1)
            self.assertEqual(records[0].record_type, "idea")
            self.assertEqual(records[0].title, "Desktop 控制台")

    def test_list_logs(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            logs_dir = home / ".agentic" / "logs"
            logs_dir.mkdir(parents=True, exist_ok=True)

            payload = {
                "meta": {
                    "timestamp": "2026-02-06T10:00:00",
                    "event_id": "abc",
                },
                "task": {"intent": "capture_idea", "status": "SUCCESS"},
                "data": {"title": "test"},
            }
            with open(logs_dir / "sample.json", "w", encoding="utf-8") as f:
                json.dump(payload, f)

            repo = CentralBrainRepository(home)
            logs = repo.list_logs()
            self.assertEqual(len(logs), 1)
            self.assertEqual(logs[0].task_intent, "capture_idea")

    def test_detect_central_home_from_records_subdir(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            (home / "records" / "worklogs").mkdir(parents=True, exist_ok=True)
            (home / ".agentic" / "logs").mkdir(parents=True, exist_ok=True)

            selected = home / "records" / "worklogs"
            detected = detect_central_home(selected)
            self.assertEqual(detected.resolve(), home.resolve())

    def test_list_records_ignores_non_record_directories(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            (home / "records" / "ideas").mkdir(parents=True, exist_ok=True)
            (home / "records" / ".obsidian").mkdir(parents=True, exist_ok=True)

            with open(home / "records" / "ideas" / "ok.json", "w", encoding="utf-8") as f:
                json.dump(
                    {
                        "type": "idea",
                        "title": "real record",
                        "created_at": "2026-02-06T10:00:00",
                        "source_text": "src",
                        "final_body": "body",
                    },
                    f,
                )

            with open(home / "records" / ".obsidian" / "noise.json", "w", encoding="utf-8") as f:
                json.dump({"foo": "bar"}, f)

            repo = CentralBrainRepository(home)
            records = repo.list_records()
            self.assertEqual(len(records), 1)
            self.assertEqual(records[0].title, "real record")


if __name__ == "__main__":
    unittest.main()
