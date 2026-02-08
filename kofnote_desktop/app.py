from __future__ import annotations

import json
import os
from datetime import datetime
from pathlib import Path
import tkinter as tk
from tkinter import filedialog, messagebox, ttk

from .ai import Analyzer
from .analytics import compute_dashboard_stats
from .models import Record, normalized_record_type
from .repository import CentralBrainRepository, detect_central_home


CONFIG_DIR = Path.home() / ".kofnote-desktop"
CONFIG_FILE = CONFIG_DIR / "config.json"


class KofNoteDesktopApp(tk.Tk):
    def __init__(self) -> None:
        super().__init__()
        self.title("KOF Note Desktop")
        self.geometry("1320x860")
        self.minsize(1080, 700)

        self.repo: CentralBrainRepository | None = None
        self.records: list[Record] = []
        self.filtered_records: list[Record] = []
        self.logs = []
        self.selected_record: Record | None = None

        self.analyzer = Analyzer()
        self.config_data = self._load_config()

        self._build_layout()
        self._hydrate_from_config()

    def _build_layout(self) -> None:
        self._build_top_bar()

        self.notebook = ttk.Notebook(self)
        self.notebook.pack(fill="both", expand=True, padx=8, pady=(0, 8))

        self.dashboard_tab = ttk.Frame(self.notebook)
        self.records_tab = ttk.Frame(self.notebook)
        self.logs_tab = ttk.Frame(self.notebook)
        self.ai_tab = ttk.Frame(self.notebook)

        self.notebook.add(self.dashboard_tab, text="Dashboard")
        self.notebook.add(self.records_tab, text="Records")
        self.notebook.add(self.logs_tab, text="Central Logs")
        self.notebook.add(self.ai_tab, text="AI Analysis")

        self._build_dashboard_tab()
        self._build_records_tab()
        self._build_logs_tab()
        self._build_ai_tab()

    def _build_top_bar(self) -> None:
        frame = ttk.Frame(self)
        frame.pack(fill="x", padx=8, pady=8)

        ttk.Label(frame, text="Central Home:").pack(side="left")

        self.central_home_var = tk.StringVar()
        home_entry = ttk.Entry(frame, textvariable=self.central_home_var)
        home_entry.pack(side="left", fill="x", expand=True, padx=(8, 8))

        ttk.Button(frame, text="選擇資料夾", command=self._choose_central_home).pack(side="left")
        ttk.Button(frame, text="Reload", command=self._reload_data).pack(side="left", padx=(8, 0))

    def _build_dashboard_tab(self) -> None:
        stats_header = ttk.Frame(self.dashboard_tab)
        stats_header.pack(fill="x", padx=10, pady=10)

        self.total_records_var = tk.StringVar(value="0")
        self.total_logs_var = tk.StringVar(value="0")
        self.pending_sync_var = tk.StringVar(value="0")

        for label_text, var in [
            ("Records", self.total_records_var),
            ("Logs", self.total_logs_var),
            ("Pending Sync", self.pending_sync_var),
        ]:
            card = ttk.Frame(stats_header, relief="ridge", padding=12)
            card.pack(side="left", padx=6)
            ttk.Label(card, text=label_text).pack(anchor="w")
            ttk.Label(card, textvariable=var, font=("TkDefaultFont", 16, "bold")).pack(anchor="w")

        split = ttk.PanedWindow(self.dashboard_tab, orient="horizontal")
        split.pack(fill="both", expand=True, padx=10, pady=(0, 10))

        left = ttk.Frame(split, padding=8)
        right = ttk.Frame(split, padding=8)
        split.add(left, weight=1)
        split.add(right, weight=1)

        ttk.Label(left, text="Record Type 分布", font=("TkDefaultFont", 11, "bold")).pack(anchor="w")
        self.type_counts_text = tk.Text(left, height=14, wrap="word")
        self.type_counts_text.pack(fill="both", expand=True, pady=(6, 0))
        self.type_counts_text.configure(state="disabled")

        ttk.Label(right, text="近 7 天活動", font=("TkDefaultFont", 11, "bold")).pack(anchor="w")
        self.recent_tree = ttk.Treeview(
            right,
            columns=("day", "count"),
            show="headings",
            height=7,
        )
        self.recent_tree.heading("day", text="Date")
        self.recent_tree.heading("count", text="Count")
        self.recent_tree.column("day", width=140)
        self.recent_tree.column("count", width=90)
        self.recent_tree.pack(fill="x", pady=(6, 10))

        ttk.Label(right, text="熱門 Tags", font=("TkDefaultFont", 11, "bold")).pack(anchor="w")
        self.top_tags_tree = ttk.Treeview(
            right,
            columns=("tag", "count"),
            show="headings",
            height=7,
        )
        self.top_tags_tree.heading("tag", text="Tag")
        self.top_tags_tree.heading("count", text="Count")
        self.top_tags_tree.column("tag", width=180)
        self.top_tags_tree.column("count", width=90)
        self.top_tags_tree.pack(fill="both", expand=True, pady=(6, 0))

    def _build_records_tab(self) -> None:
        toolbar = ttk.Frame(self.records_tab)
        toolbar.pack(fill="x", padx=10, pady=(10, 4))

        ttk.Button(toolbar, text="New", command=self._new_record).pack(side="left")
        ttk.Button(toolbar, text="Save", command=self._save_record).pack(side="left", padx=(6, 0))
        ttk.Button(toolbar, text="Delete", command=self._delete_record).pack(side="left", padx=(6, 0))

        ttk.Label(toolbar, text="Type").pack(side="left", padx=(16, 4))
        self.record_filter_type_var = tk.StringVar(value="all")
        self.record_filter_type_combo = ttk.Combobox(
            toolbar,
            textvariable=self.record_filter_type_var,
            width=10,
            state="readonly",
            values=["all", "decision", "worklog", "idea", "backlog", "note"],
        )
        self.record_filter_type_combo.pack(side="left")
        self.record_filter_type_combo.bind("<<ComboboxSelected>>", lambda _event: self._apply_record_filter())

        ttk.Label(toolbar, text="搜尋").pack(side="left", padx=(12, 4))
        self.record_search_var = tk.StringVar()
        search_entry = ttk.Entry(toolbar, textvariable=self.record_search_var, width=28)
        search_entry.pack(side="left")
        search_entry.bind("<Return>", lambda _event: self._apply_record_filter())
        ttk.Button(toolbar, text="Filter", command=self._apply_record_filter).pack(side="left", padx=(6, 0))

        split = ttk.PanedWindow(self.records_tab, orient="horizontal")
        split.pack(fill="both", expand=True, padx=10, pady=(0, 10))

        left = ttk.Frame(split)
        right = ttk.Frame(split, padding=8)
        split.add(left, weight=1)
        split.add(right, weight=2)

        self.record_listbox = tk.Listbox(left)
        self.record_listbox.pack(side="left", fill="both", expand=True)
        list_scroll = ttk.Scrollbar(left, orient="vertical", command=self.record_listbox.yview)
        list_scroll.pack(side="right", fill="y")
        self.record_listbox.configure(yscrollcommand=list_scroll.set)
        self.record_listbox.bind("<<ListboxSelect>>", self._on_record_select)

        self.record_type_var = tk.StringVar(value="idea")
        self.record_title_var = tk.StringVar()
        self.record_created_at_var = tk.StringVar()
        self.record_date_var = tk.StringVar()
        self.record_tags_var = tk.StringVar()
        self.record_notion_url_var = tk.StringVar()
        self.record_sync_status_var = tk.StringVar(value="SUCCESS")
        self.record_json_path_var = tk.StringVar(value="-")
        self.record_md_path_var = tk.StringVar(value="-")

        form = ttk.Frame(right)
        form.pack(fill="both", expand=True)

        row = 0
        row = self._form_row_combo(
            form,
            row,
            "Type",
            self.record_type_var,
            ["decision", "worklog", "idea", "backlog", "note"],
            width=14,
        )
        row = self._form_row_entry(form, row, "Title", self.record_title_var)
        row = self._form_row_entry(form, row, "Created At (ISO)", self.record_created_at_var)
        row = self._form_row_entry(form, row, "Date (YYYY-MM-DD)", self.record_date_var)
        row = self._form_row_entry(form, row, "Tags (comma)", self.record_tags_var)
        row = self._form_row_entry(form, row, "Notion URL", self.record_notion_url_var)
        row = self._form_row_combo(
            form,
            row,
            "Sync Status",
            self.record_sync_status_var,
            ["SUCCESS", "PENDING", "FAILED"],
            width=14,
        )
        row = self._form_row_entry(form, row, "JSON Path", self.record_json_path_var, readonly=True)
        row = self._form_row_entry(form, row, "Markdown Path", self.record_md_path_var, readonly=True)

        ttk.Label(form, text="Final Body").grid(row=row, column=0, sticky="nw", pady=(8, 4))
        self.record_body_text = tk.Text(form, height=12, wrap="word")
        self.record_body_text.grid(row=row, column=1, sticky="nsew", pady=(8, 4))
        row += 1

        ttk.Label(form, text="Source Text").grid(row=row, column=0, sticky="nw", pady=(8, 4))
        self.record_source_text = tk.Text(form, height=8, wrap="word")
        self.record_source_text.grid(row=row, column=1, sticky="nsew", pady=(8, 4))

        form.columnconfigure(1, weight=1)
        form.rowconfigure(row - 1, weight=1)
        form.rowconfigure(row, weight=1)

        self._new_record()

    def _build_logs_tab(self) -> None:
        wrapper = ttk.PanedWindow(self.logs_tab, orient="horizontal")
        wrapper.pack(fill="both", expand=True, padx=10, pady=10)

        left = ttk.Frame(wrapper)
        right = ttk.Frame(wrapper)
        wrapper.add(left, weight=2)
        wrapper.add(right, weight=3)

        self.logs_tree = ttk.Treeview(
            left,
            columns=("timestamp", "intent", "status", "title"),
            show="headings",
        )
        for column, heading, width in [
            ("timestamp", "Timestamp", 180),
            ("intent", "Intent", 180),
            ("status", "Status", 90),
            ("title", "Title", 240),
        ]:
            self.logs_tree.heading(column, text=heading)
            self.logs_tree.column(column, width=width, stretch=True)

        self.logs_tree.pack(side="left", fill="both", expand=True)
        log_scroll = ttk.Scrollbar(left, orient="vertical", command=self.logs_tree.yview)
        log_scroll.pack(side="right", fill="y")
        self.logs_tree.configure(yscrollcommand=log_scroll.set)
        self.logs_tree.bind("<<TreeviewSelect>>", self._on_log_select)

        self.log_detail = tk.Text(right, wrap="word")
        self.log_detail.pack(fill="both", expand=True)

    def _build_ai_tab(self) -> None:
        toolbar = ttk.Frame(self.ai_tab)
        toolbar.pack(fill="x", padx=10, pady=10)

        ttk.Label(toolbar, text="Model").pack(side="left")
        self.ai_model_var = tk.StringVar(value=self.config_data.get("openai_model", "gpt-4.1-mini"))
        ttk.Entry(toolbar, textvariable=self.ai_model_var, width=18).pack(side="left", padx=(6, 14))

        ttk.Label(toolbar, text="API Key").pack(side="left")
        self.ai_api_key_var = tk.StringVar(value=self.config_data.get("openai_api_key", ""))
        ttk.Entry(toolbar, textvariable=self.ai_api_key_var, width=32, show="*").pack(side="left", padx=(6, 10))

        ttk.Button(toolbar, text="Local Analysis", command=self._run_local_analysis).pack(side="left")
        ttk.Button(toolbar, text="OpenAI Analysis", command=self._run_openai_analysis).pack(side="left", padx=(8, 0))

        prompt_frame = ttk.LabelFrame(self.ai_tab, text="Prompt", padding=8)
        prompt_frame.pack(fill="x", padx=10, pady=(0, 8))
        self.ai_prompt_text = tk.Text(prompt_frame, height=5, wrap="word")
        self.ai_prompt_text.pack(fill="x")
        self.ai_prompt_text.insert(
            "1.0",
            "請整理最近的重點方向、重複模式、風險，並產生下一週可執行清單。",
        )

        output_frame = ttk.LabelFrame(self.ai_tab, text="Output", padding=8)
        output_frame.pack(fill="both", expand=True, padx=10, pady=(0, 10))
        self.ai_output_text = tk.Text(output_frame, wrap="word")
        self.ai_output_text.pack(fill="both", expand=True)

    def _form_row_entry(
        self,
        parent: ttk.Frame,
        row: int,
        label: str,
        variable: tk.StringVar,
        readonly: bool = False,
    ) -> int:
        ttk.Label(parent, text=label).grid(row=row, column=0, sticky="w", pady=4, padx=(0, 8))
        entry_state = "readonly" if readonly else "normal"
        entry = ttk.Entry(parent, textvariable=variable, state=entry_state)
        entry.grid(row=row, column=1, sticky="ew", pady=4)
        return row + 1

    def _form_row_combo(
        self,
        parent: ttk.Frame,
        row: int,
        label: str,
        variable: tk.StringVar,
        values: list[str],
        width: int,
    ) -> int:
        ttk.Label(parent, text=label).grid(row=row, column=0, sticky="w", pady=4, padx=(0, 8))
        combo = ttk.Combobox(parent, textvariable=variable, values=values, state="readonly", width=width)
        combo.grid(row=row, column=1, sticky="w", pady=4)
        return row + 1

    def _hydrate_from_config(self) -> None:
        default_home = (
            self.config_data.get("central_home")
            or os.getenv("ANTIGRAVITY_LOG_HOME")
            or ""
        )

        if default_home:
            self.central_home_var.set(default_home)
            self._set_central_home(Path(default_home), silent=True)
        else:
            self._new_record()

    def _choose_central_home(self) -> None:
        initial = self.central_home_var.get().strip() or str(Path.home())
        selected = filedialog.askdirectory(initialdir=initial)
        if not selected:
            return
        self.central_home_var.set(selected)
        self._set_central_home(Path(selected), silent=False)

    def _set_central_home(self, candidate: Path, silent: bool) -> None:
        try:
            original = candidate.expanduser().resolve()
            resolved = detect_central_home(candidate)
            self.repo = CentralBrainRepository(resolved)
            self.repo.ensure_structure()
            self.central_home_var.set(str(resolved))
            self.config_data["central_home"] = str(resolved)
            self._save_config()
            self._reload_data()
            if not silent:
                if original != resolved:
                    messagebox.showinfo(
                        "KOF Note",
                        "偵測到你選的是子資料夾，已自動切換到 Central Home:\n"
                        f"{resolved}",
                    )
                else:
                    messagebox.showinfo("KOF Note", f"Central Home loaded:\n{resolved}")
        except Exception as error:
            if not silent:
                messagebox.showerror("KOF Note", f"Cannot load central home:\n{error}")

    def _reload_data(self) -> None:
        if not self.repo:
            path_text = self.central_home_var.get().strip()
            if path_text:
                self._set_central_home(Path(path_text), silent=False)
            return

        self.records = self.repo.list_records()
        self.logs = self.repo.list_logs()

        self._apply_record_filter()
        self._refresh_dashboard()
        self._refresh_logs()

    def _apply_record_filter(self) -> None:
        selected_type = self.record_filter_type_var.get().strip().lower()
        keyword = self.record_search_var.get().strip().lower()

        filtered: list[Record] = []
        for record in self.records:
            if selected_type != "all" and record.record_type != selected_type:
                continue

            if keyword:
                text = " ".join(
                    [
                        record.title,
                        record.final_body,
                        record.source_text,
                        " ".join(record.tags),
                    ]
                ).lower()
                if keyword not in text:
                    continue
            filtered.append(record)

        self.filtered_records = filtered
        self.record_listbox.delete(0, tk.END)
        for record in filtered:
            created = record.created_at[:19]
            self.record_listbox.insert(tk.END, f"{created} | {record.record_type:<8} | {record.title}")

    def _refresh_dashboard(self) -> None:
        stats = compute_dashboard_stats(self.records, self.logs)
        self.total_records_var.set(str(stats.total_records))
        self.total_logs_var.set(str(stats.total_logs))
        self.pending_sync_var.set(str(stats.pending_sync_count))

        lines = []
        for record_type in ["decision", "worklog", "idea", "backlog", "note"]:
            lines.append(f"{record_type:<8} : {stats.type_counts.get(record_type, 0)}")

        self.type_counts_text.configure(state="normal")
        self.type_counts_text.delete("1.0", tk.END)
        self.type_counts_text.insert("1.0", "\n".join(lines))
        self.type_counts_text.configure(state="disabled")

        for tree in [self.recent_tree, self.top_tags_tree]:
            for row_id in tree.get_children():
                tree.delete(row_id)

        for day, count in stats.recent_daily_counts.items():
            self.recent_tree.insert("", tk.END, values=(day, count))

        for tag, count in stats.top_tags:
            self.top_tags_tree.insert("", tk.END, values=(tag, count))

    def _refresh_logs(self) -> None:
        for row_id in self.logs_tree.get_children():
            self.logs_tree.delete(row_id)

        for idx, item in enumerate(self.logs):
            self.logs_tree.insert(
                "",
                tk.END,
                iid=str(idx),
                values=(item.timestamp, item.task_intent, item.status, item.title),
            )

        self.log_detail.delete("1.0", tk.END)

    def _on_record_select(self, _event: tk.Event) -> None:
        selected = self.record_listbox.curselection()
        if not selected:
            return
        index = selected[0]
        if index >= len(self.filtered_records):
            return

        record = self.filtered_records[index]
        self.selected_record = record
        self._bind_record(record)

    def _bind_record(self, record: Record) -> None:
        self.record_type_var.set(record.record_type)
        self.record_title_var.set(record.title)
        self.record_created_at_var.set(record.created_at)
        self.record_date_var.set(record.date or "")
        self.record_tags_var.set(", ".join(record.tags))
        self.record_notion_url_var.set(record.notion_url or "")
        self.record_sync_status_var.set(record.notion_sync_status or "SUCCESS")
        self.record_json_path_var.set(str(record.json_path) if record.json_path else "-")
        self.record_md_path_var.set(str(record.md_path) if record.md_path else "-")

        self.record_body_text.delete("1.0", tk.END)
        self.record_body_text.insert("1.0", record.final_body)

        self.record_source_text.delete("1.0", tk.END)
        self.record_source_text.insert("1.0", record.source_text)

    def _new_record(self) -> None:
        self.selected_record = None
        self.record_type_var.set("idea")
        self.record_title_var.set("")
        self.record_created_at_var.set(datetime.now().isoformat(timespec="seconds"))
        self.record_date_var.set(datetime.now().date().isoformat())
        self.record_tags_var.set("")
        self.record_notion_url_var.set("")
        self.record_sync_status_var.set("SUCCESS")
        self.record_json_path_var.set("-")
        self.record_md_path_var.set("-")
        self.record_body_text.delete("1.0", tk.END)
        self.record_source_text.delete("1.0", tk.END)

    def _collect_form_record(self) -> Record:
        tags = [tag.strip() for tag in self.record_tags_var.get().split(",") if tag.strip()]
        record_type = normalized_record_type(self.record_type_var.get())

        return Record(
            record_type=record_type,
            title=self.record_title_var.get().strip() or "Untitled",
            created_at=self.record_created_at_var.get().strip()
            or datetime.now().isoformat(timespec="seconds"),
            source_text=self.record_source_text.get("1.0", tk.END).strip(),
            final_body=self.record_body_text.get("1.0", tk.END).strip(),
            tags=tags,
            date=self.record_date_var.get().strip() or None,
            notion_page_id=self.selected_record.notion_page_id if self.selected_record else None,
            notion_url=self.record_notion_url_var.get().strip() or None,
            notion_sync_status=self.record_sync_status_var.get().strip() or "SUCCESS",
            notion_error=self.selected_record.notion_error if self.selected_record else None,
            json_path=self.selected_record.json_path if self.selected_record else None,
            md_path=self.selected_record.md_path if self.selected_record else None,
        )

    def _save_record(self) -> None:
        if not self.repo:
            messagebox.showwarning("KOF Note", "請先選擇 Central Home")
            return

        try:
            record = self._collect_form_record()
            existing_path = self.selected_record.json_path if self.selected_record else None
            saved = self.repo.save_record(record, existing_json_path=existing_path)
            self._reload_data()
            self._focus_record_by_path(saved.json_path)
            messagebox.showinfo("KOF Note", "Record saved")
        except Exception as error:
            messagebox.showerror("KOF Note", f"Save failed:\n{error}")

    def _delete_record(self) -> None:
        if not self.repo or not self.selected_record:
            return

        confirm = messagebox.askyesno("KOF Note", "確定刪除這筆記錄？")
        if not confirm:
            return

        try:
            self.repo.delete_record(self.selected_record)
            self._reload_data()
            self._new_record()
        except Exception as error:
            messagebox.showerror("KOF Note", f"Delete failed:\n{error}")

    def _focus_record_by_path(self, json_path: Path | None) -> None:
        if not json_path:
            return

        for index, record in enumerate(self.filtered_records):
            if record.json_path == json_path:
                self.record_listbox.selection_clear(0, tk.END)
                self.record_listbox.selection_set(index)
                self.record_listbox.see(index)
                self._bind_record(record)
                self.selected_record = record
                break

    def _on_log_select(self, _event: tk.Event) -> None:
        selected = self.logs_tree.selection()
        if not selected:
            return

        index = int(selected[0])
        if index >= len(self.logs):
            return

        item = self.logs[index]
        pretty = json.dumps(item.raw, ensure_ascii=False, indent=2)
        self.log_detail.delete("1.0", tk.END)
        self.log_detail.insert("1.0", pretty)

    def _run_local_analysis(self) -> None:
        prompt = self.ai_prompt_text.get("1.0", tk.END).strip()
        stats = compute_dashboard_stats(self.records, self.logs)
        result = self.analyzer.local_analysis(stats, self.records, self.logs, prompt)
        self._render_ai_result(result.content)

    def _run_openai_analysis(self) -> None:
        prompt = self.ai_prompt_text.get("1.0", tk.END).strip()
        model = self.ai_model_var.get().strip() or "gpt-4.1-mini"
        api_key = self.ai_api_key_var.get().strip()

        self.config_data["openai_model"] = model
        self.config_data["openai_api_key"] = api_key
        self._save_config()

        result = self.analyzer.openai_analysis(
            self.records,
            self.logs,
            prompt,
            api_key=api_key,
            model=model,
        )
        if result.ok:
            self._render_ai_result(result.content)
        else:
            self._render_ai_result(f"[ERROR]\n{result.error}")

    def _render_ai_result(self, content: str) -> None:
        self.ai_output_text.delete("1.0", tk.END)
        self.ai_output_text.insert("1.0", content)

    def _load_config(self) -> dict:
        if not CONFIG_FILE.exists():
            return {}
        try:
            with open(CONFIG_FILE, "r", encoding="utf-8") as f:
                data = json.load(f)
            return data if isinstance(data, dict) else {}
        except Exception:
            return {}

    def _save_config(self) -> None:
        CONFIG_DIR.mkdir(parents=True, exist_ok=True)
        with open(CONFIG_FILE, "w", encoding="utf-8") as f:
            json.dump(self.config_data, f, ensure_ascii=False, indent=2)
