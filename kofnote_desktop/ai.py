from __future__ import annotations

import json
import os
import urllib.error
import urllib.request
from dataclasses import dataclass

from .analytics import DashboardStats, build_context_digest
from .models import CentralLogEntry, Record


@dataclass
class AIResult:
    ok: bool
    content: str
    error: str | None = None


class Analyzer:
    def local_analysis(
        self,
        stats: DashboardStats,
        records: list[Record],
        logs: list[CentralLogEntry],
        user_prompt: str,
    ) -> AIResult:
        top_type = "-"
        if stats.type_counts:
            top_type = max(stats.type_counts, key=stats.type_counts.get)

        lines = [
            "# KOF Local Analysis",
            "",
            f"- Total records: {stats.total_records}",
            f"- Total logs: {stats.total_logs}",
            f"- Pending sync: {stats.pending_sync_count}",
            f"- Dominant record type: {top_type}",
            "",
            "## Top Tags",
        ]

        if stats.top_tags:
            for tag, count in stats.top_tags[:5]:
                lines.append(f"- {tag}: {count}")
        else:
            lines.append("- (no tags yet)")

        lines.extend(["", "## Recent 7 Days Activity"])
        for day, count in stats.recent_daily_counts.items():
            lines.append(f"- {day}: {count}")

        lines.extend(
            [
                "",
                "## Suggested Actions",
                "- Consolidate related backlog items into a weekly execution list.",
                "- Convert repeated worklog themes into reusable playbooks.",
                "- Review pending Notion sync records and retry them.",
                "",
                "## Prompt Focus",
                user_prompt.strip() or "(none)",
            ]
        )

        return AIResult(ok=True, content="\n".join(lines))

    def openai_analysis(
        self,
        records: list[Record],
        logs: list[CentralLogEntry],
        user_prompt: str,
        api_key: str | None = None,
        model: str = "gpt-4.1-mini",
        base_url: str = "https://api.openai.com/v1/responses",
    ) -> AIResult:
        key = api_key or os.getenv("OPENAI_API_KEY")
        if not key:
            return AIResult(
                ok=False,
                content="",
                error="Missing OPENAI_API_KEY. Add it in the app or environment.",
            )

        digest = build_context_digest(records, logs)
        prompt = (
            "You are analyzing a personal central log. "
            "Output concise sections: Summary, Patterns, Risks, Next 7 Days Action Plan.\n\n"
            f"User request:\n{user_prompt or '(none)'}\n\n"
            f"Context:\n{digest}"
        )

        payload = {
            "model": model,
            "input": [
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "input_text",
                            "text": prompt,
                        }
                    ],
                }
            ],
        }

        try:
            request = urllib.request.Request(
                base_url,
                data=json.dumps(payload).encode("utf-8"),
                headers={
                    "Authorization": f"Bearer {key}",
                    "Content-Type": "application/json",
                },
                method="POST",
            )
            with urllib.request.urlopen(request, timeout=45) as response:
                body = response.read().decode("utf-8")
            data = json.loads(body)
            text = self._extract_response_text(data)
            if not text:
                return AIResult(
                    ok=False,
                    content="",
                    error="OpenAI response did not include readable text.",
                )
            return AIResult(ok=True, content=text)
        except urllib.error.HTTPError as error:
            detail = error.read().decode("utf-8", errors="ignore")
            return AIResult(ok=False, content="", error=f"HTTP {error.code}: {detail}")
        except Exception as error:
            return AIResult(ok=False, content="", error=str(error))

    @staticmethod
    def _extract_response_text(payload: dict) -> str:
        if isinstance(payload.get("output_text"), str) and payload["output_text"].strip():
            return payload["output_text"].strip()

        output = payload.get("output")
        if not isinstance(output, list):
            return ""

        chunks: list[str] = []
        for item in output:
            content = item.get("content") if isinstance(item, dict) else None
            if not isinstance(content, list):
                continue
            for block in content:
                if not isinstance(block, dict):
                    continue
                if block.get("type") in {"output_text", "text"} and isinstance(
                    block.get("text"), str
                ):
                    chunks.append(block["text"])
        return "\n".join(chunk.strip() for chunk in chunks if chunk.strip())
