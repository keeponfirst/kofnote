import unittest

from kofnote_desktop.analytics import compute_dashboard_stats
from kofnote_desktop.models import CentralLogEntry, Record


class AnalyticsTest(unittest.TestCase):
    def test_compute_dashboard_stats(self) -> None:
        records = [
            Record(
                record_type="idea",
                title="A",
                created_at="2026-02-05T09:00:00",
                source_text="",
                final_body="",
                tags=["ai", "kof"],
                notion_sync_status="SUCCESS",
            ),
            Record(
                record_type="backlog",
                title="B",
                created_at="2026-02-06T10:00:00",
                source_text="",
                final_body="",
                tags=["ai"],
                notion_sync_status="PENDING",
            ),
        ]

        logs = [
            CentralLogEntry(
                timestamp="2026-02-06T11:00:00",
                event_id="1",
                task_intent="capture_backlog",
                status="SUCCESS",
                title="B",
                data={},
                raw={},
            )
        ]

        stats = compute_dashboard_stats(records, logs)
        self.assertEqual(stats.total_records, 2)
        self.assertEqual(stats.total_logs, 1)
        self.assertEqual(stats.type_counts.get("idea"), 1)
        self.assertEqual(stats.type_counts.get("backlog"), 1)
        self.assertEqual(stats.pending_sync_count, 1)
        self.assertEqual(stats.top_tags[0][0], "ai")


if __name__ == "__main__":
    unittest.main()
