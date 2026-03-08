import type { PromptTemplate } from '../types'

/**
 * 預設角色 Prompt 模板
 * 參考 prompts.stingtao.info 及 prompts.chat 社群模板設計
 * 每個模板使用 {{display_name}}, {{role}}, {{company}}, {{department}}, {{bio}} 作為身份變數
 */
export const DEFAULT_PROMPT_TEMPLATES: Omit<PromptTemplate, 'id' | 'createdAt' | 'updatedAt'>[] = [
  // ── 工作效率 ──────────────────────────────────────────
  {
    name: '每日工作日報',
    description: '撰寫今日工作重點與進度的日報',
    content:
      '我是 {{display_name}}，{{role}} at {{company}}（{{department}}）。\n{{bio}}\n\n今日工作重點：\n{{focus}}\n\n請幫我撰寫一份簡潔清晰的工作日報，包含完成事項、進行中事項、遇到的問題。',
    variables: [{ key: 'focus', label: '今日重點', placeholder: '請描述今日主要工作內容' }],
  },
  {
    name: '週報撰寫',
    description: '彙整本週工作成果與下週計畫',
    content:
      '我是 {{display_name}}，{{role}} at {{company}}（{{department}}）。\n{{bio}}\n\n本週完成事項：\n{{accomplishments}}\n\n遇到的挑戰：\n{{challenges}}\n\n請幫我撰寫一份專業的週報，包含本週成果摘要、問題與解決方案、下週計畫建議。格式清晰、重點分明。',
    variables: [
      { key: 'accomplishments', label: '本週成果', placeholder: '列出本週完成的主要工作' },
      { key: 'challenges', label: '遇到的挑戰', placeholder: '描述本週遇到的問題或阻礙' },
    ],
  },
  {
    name: '會議記錄整理',
    description: '將會議筆記整理為結構化的會議記錄',
    content:
      '我是 {{display_name}}，{{role}} at {{company}}（{{department}}）。\n\n以下是會議的原始筆記：\n{{raw_notes}}\n\n會議主題：{{meeting_topic}}\n\n請幫我整理為正式的會議記錄，包含：\n1. 會議摘要\n2. 討論要點\n3. 決議事項\n4. 行動項目（Action Items）與負責人\n5. 後續追蹤事項',
    variables: [
      { key: 'meeting_topic', label: '會議主題', placeholder: '例：Q1 產品規劃會議' },
      { key: 'raw_notes', label: '原始筆記', placeholder: '貼上會議中的筆記內容' },
    ],
  },

  // ── 專業信件與溝通 ──────────────────────────────────────
  {
    name: '專業信件撰寫',
    description: '撰寫正式商業信件或電子郵件',
    content:
      '我是 {{display_name}}，{{role}} at {{company}}（{{department}}）。\n{{bio}}\n\n收件人：{{recipient}}\n信件目的：{{purpose}}\n關鍵要點：{{key_points}}\n\n請幫我撰寫一封專業、禮貌的商業信件。語氣正式但不生硬，重點清晰。',
    variables: [
      { key: 'recipient', label: '收件人', placeholder: '例：客戶 / 主管 / 合作夥伴' },
      { key: 'purpose', label: '信件目的', placeholder: '例：專案進度更新 / 合作邀請 / 問題回報' },
      { key: 'key_points', label: '關鍵要點', placeholder: '列出需要在信件中提到的重點' },
    ],
  },
  {
    name: '客戶溝通回覆',
    description: '撰寫客戶問題的專業回覆',
    content:
      '我是 {{display_name}}，{{role}} at {{company}}（{{department}}）。\n{{bio}}\n\n客戶問題/訊息：\n{{customer_message}}\n\n背景資訊：{{context}}\n\n請幫我撰寫一份專業且友善的回覆，確保：\n1. 正面回應客戶的問題\n2. 提供清晰的解決方案或說明\n3. 保持品牌專業形象\n4. 適當的結尾與後續步驟',
    variables: [
      { key: 'customer_message', label: '客戶訊息', placeholder: '貼上客戶的問題或訊息' },
      { key: 'context', label: '背景資訊', placeholder: '提供相關背景，例如產品版本、已知問題等' },
    ],
  },

  // ── 技術開發 ──────────────────────────────────────────
  {
    name: '程式碼審查',
    description: '對程式碼進行專業的 Code Review',
    content:
      '我是 {{display_name}}，{{role}} at {{company}}（{{department}}）。\n{{bio}}\n\n請對以下程式碼進行專業的 Code Review：\n\n語言/框架：{{tech_stack}}\n\n```\n{{code}}\n```\n\n請從以下面向分析：\n1. 程式碼品質與可讀性\n2. 潛在的 Bug 或邊界情況\n3. 效能考量\n4. 安全性問題\n5. 最佳實踐建議\n\n提供具體的改善建議與修改範例。',
    variables: [
      { key: 'tech_stack', label: '技術棧', placeholder: '例：TypeScript + React / Rust / Python' },
      { key: 'code', label: '程式碼', placeholder: '貼上要審查的程式碼' },
    ],
  },
  {
    name: '技術文件撰寫',
    description: '撰寫 API 文件、架構說明或技術規格書',
    content:
      '我是 {{display_name}}，{{role}} at {{company}}（{{department}}）。\n{{bio}}\n\n文件類型：{{doc_type}}\n主題：{{topic}}\n目標讀者：{{audience}}\n技術細節：\n{{details}}\n\n請幫我撰寫一份清晰的技術文件，包含：\n1. 概述與目的\n2. 架構/流程說明\n3. 使用方式與範例\n4. 注意事項與限制\n5. 參考資料',
    variables: [
      { key: 'doc_type', label: '文件類型', placeholder: '例：API 文件 / 架構設計 / 使用手冊' },
      { key: 'topic', label: '主題', placeholder: '文件的主要主題' },
      { key: 'audience', label: '目標讀者', placeholder: '例：前端工程師 / 新進成員 / 產品經理' },
      { key: 'details', label: '技術細節', placeholder: '提供需要涵蓋的技術細節' },
    ],
  },
  {
    name: 'Bug 分析與除錯',
    description: '分析 Bug 症狀並提供除錯建議',
    content:
      '我是 {{display_name}}，{{role}} at {{company}}（{{department}}）。\n{{bio}}\n\n問題描述：\n{{bug_description}}\n\n錯誤訊息/日誌：\n{{error_log}}\n\n環境資訊：{{environment}}\n\n請幫我：\n1. 分析可能的根本原因\n2. 提供除錯步驟建議\n3. 列出可能的解決方案\n4. 建議預防措施',
    variables: [
      { key: 'bug_description', label: '問題描述', placeholder: '描述 Bug 的行為與預期行為' },
      { key: 'error_log', label: '錯誤訊息', placeholder: '貼上相關的錯誤訊息或日誌' },
      { key: 'environment', label: '環境資訊', placeholder: '例：Node 20 / macOS / Chrome 130' },
    ],
  },

  // ── 商業分析與策略 ──────────────────────────────────────
  {
    name: 'SWOT 分析',
    description: '對產品、專案或策略進行 SWOT 分析',
    content:
      '我是 {{display_name}}，{{role}} at {{company}}（{{department}}）。\n{{bio}}\n\n分析對象：{{subject}}\n背景說明：{{background}}\n\n請進行完整的 SWOT 分析：\n- **Strengths（優勢）**：內部有利因素\n- **Weaknesses（劣勢）**：內部不利因素\n- **Opportunities（機會）**：外部有利因素\n- **Threats（威脅）**：外部不利因素\n\n最後請提供策略建議，說明如何利用優勢把握機會、如何改善劣勢應對威脅。',
    variables: [
      { key: 'subject', label: '分析對象', placeholder: '例：新產品線 / 競爭策略 / 市場進入' },
      { key: 'background', label: '背景說明', placeholder: '提供相關的背景脈絡和現況' },
    ],
  },
  {
    name: '決策分析',
    description: '協助進行多方案比較與決策',
    content:
      '我是 {{display_name}}，{{role}} at {{company}}（{{department}}）。\n{{bio}}\n\n決策問題：{{decision}}\n\n候選方案：\n{{options}}\n\n考量因素：{{criteria}}\n\n請幫我進行結構化的決策分析：\n1. 各方案優缺點比較\n2. 風險評估\n3. 成本效益分析\n4. 推薦方案及理由\n5. 實施建議',
    variables: [
      { key: 'decision', label: '決策問題', placeholder: '描述需要做的決定' },
      { key: 'options', label: '候選方案', placeholder: '列出可選的方案（A、B、C...）' },
      { key: 'criteria', label: '考量因素', placeholder: '例：成本、時程、品質、風險...' },
    ],
  },
  {
    name: '專案規劃',
    description: '規劃專案時程、里程碑和工作拆分',
    content:
      '我是 {{display_name}}，{{role}} at {{company}}（{{department}}）。\n{{bio}}\n\n專案名稱：{{project_name}}\n專案目標：{{objective}}\n預計時程：{{timeline}}\n團隊資源：{{resources}}\n\n請幫我制定專案計畫，包含：\n1. 專案範圍定義\n2. 工作拆分結構（WBS）\n3. 里程碑與關鍵交付物\n4. 風險識別與緩解策略\n5. 依賴關係與關鍵路徑',
    variables: [
      { key: 'project_name', label: '專案名稱', placeholder: '專案的名稱' },
      { key: 'objective', label: '專案目標', placeholder: '描述專案要達成的目標' },
      { key: 'timeline', label: '預計時程', placeholder: '例：3 個月 / 2026 Q2' },
      { key: 'resources', label: '團隊資源', placeholder: '例：3 位工程師、1 位設計師' },
    ],
  },

  // ── 創意與內容 ──────────────────────────────────────────
  {
    name: '腦力激盪',
    description: '針對特定主題進行創意發想',
    content:
      '我是 {{display_name}}，{{role}} at {{company}}（{{department}}）。\n{{bio}}\n\n主題：{{topic}}\n背景與限制：{{constraints}}\n\n請扮演一位創意顧問，幫我進行腦力激盪：\n1. 提出至少 10 個創意想法\n2. 對每個想法簡要說明可行性\n3. 標示出最有潛力的 3 個想法\n4. 針對最佳想法提出初步的執行方向\n\n請大膽發想，不要自我設限。',
    variables: [
      { key: 'topic', label: '主題', placeholder: '描述需要發想的主題' },
      { key: 'constraints', label: '背景與限制', placeholder: '例：預算有限 / 需在一個月內完成 / 目標用戶是...' },
    ],
  },
  {
    name: '簡報大綱',
    description: '規劃簡報的結構與內容大綱',
    content:
      '我是 {{display_name}}，{{role}} at {{company}}（{{department}}）。\n{{bio}}\n\n簡報主題：{{presentation_topic}}\n目標觀眾：{{audience}}\n簡報時長：{{duration}}\n核心訊息：{{key_message}}\n\n請幫我規劃簡報大綱：\n1. 開場（如何抓住注意力）\n2. 主體結構（3-5 個核心段落）\n3. 每段落的關鍵要點與支持數據建議\n4. 結語與 Call to Action\n5. Q&A 準備建議',
    variables: [
      { key: 'presentation_topic', label: '簡報主題', placeholder: '簡報的主要主題' },
      { key: 'audience', label: '目標觀眾', placeholder: '例：公司高層 / 投資人 / 技術團隊' },
      { key: 'duration', label: '簡報時長', placeholder: '例：15 分鐘 / 30 分鐘' },
      { key: 'key_message', label: '核心訊息', placeholder: '你希望觀眾記住的一件事' },
    ],
  },
  {
    name: '文章/部落格撰寫',
    description: '撰寫專業文章或部落格文章',
    content:
      '我是 {{display_name}}，{{role}} at {{company}}（{{department}}）。\n{{bio}}\n\n文章主題：{{article_topic}}\n目標讀者：{{target_reader}}\n文章風格：{{style}}\n\n請幫我撰寫一篇高品質的文章，要求：\n1. 吸引人的標題\n2. 引人入勝的開頭\n3. 結構清晰的主體內容\n4. 實用的觀點或建議\n5. 有力的結尾\n\n字數約 {{word_count}} 字。',
    variables: [
      { key: 'article_topic', label: '文章主題', placeholder: '文章的核心主題' },
      { key: 'target_reader', label: '目標讀者', placeholder: '例：技術人員 / 產品經理 / 一般大眾' },
      { key: 'style', label: '文章風格', placeholder: '例：專業分析 / 輕鬆分享 / 教學教程' },
      { key: 'word_count', label: '目標字數', placeholder: '例：1000 / 2000' },
    ],
  },

  // ── 學習與成長 ──────────────────────────────────────────
  {
    name: '學習筆記整理',
    description: '將學習內容整理為結構化筆記',
    content:
      '我是 {{display_name}}，{{role}} at {{company}}（{{department}}）。\n{{bio}}\n\n學習主題：{{learning_topic}}\n學習來源：{{source}}\n原始筆記：\n{{raw_notes}}\n\n請幫我整理為結構化的學習筆記：\n1. 核心概念摘要\n2. 關鍵知識點（條列式）\n3. 實際應用場景\n4. 與我現有工作的關聯\n5. 延伸學習建議',
    variables: [
      { key: 'learning_topic', label: '學習主題', placeholder: '例：Rust 所有權機制 / 系統設計模式' },
      { key: 'source', label: '學習來源', placeholder: '例：課程名稱 / 書籍 / 文章連結' },
      { key: 'raw_notes', label: '原始筆記', placeholder: '貼上你的學習筆記或重點' },
    ],
  },
  {
    name: '面試準備',
    description: '準備面試問題和回答策略',
    content:
      '我是 {{display_name}}，{{role}} at {{company}}（{{department}}）。\n{{bio}}\n\n目標職位：{{target_position}}\n目標公司：{{target_company}}\n面試類型：{{interview_type}}\n\n請幫我準備面試：\n1. 常見問題清單（至少 10 題）\n2. 每題的回答策略和範例回答\n3. STAR 法則應用建議\n4. 該職位的加分技能和知識\n5. 反問面試官的好問題',
    variables: [
      { key: 'target_position', label: '目標職位', placeholder: '例：Senior Software Engineer' },
      { key: 'target_company', label: '目標公司', placeholder: '例：Google / 新創公司' },
      { key: 'interview_type', label: '面試類型', placeholder: '例：技術面試 / 行為面試 / 系統設計' },
    ],
  },

  // ── 數據與分析 ──────────────────────────────────────────
  {
    name: '數據分析報告',
    description: '分析數據並撰寫分析報告',
    content:
      '我是 {{display_name}}，{{role}} at {{company}}（{{department}}）。\n{{bio}}\n\n分析目的：{{purpose}}\n數據摘要：\n{{data_summary}}\n\n請幫我撰寫數據分析報告：\n1. 摘要與關鍵發現\n2. 數據趨勢分析\n3. 異常點識別與解釋\n4. 與業務目標的關聯\n5. 建議行動方案\n6. 需要進一步調查的項目',
    variables: [
      { key: 'purpose', label: '分析目的', placeholder: '例：分析用戶留存率下降原因' },
      { key: 'data_summary', label: '數據摘要', placeholder: '貼上數據摘要、表格或關鍵數字' },
    ],
  },

  // ── 翻譯與語言 ──────────────────────────────────────────
  {
    name: '專業翻譯',
    description: '翻譯文件並保持專業術語準確',
    content:
      '我是 {{display_name}}，{{role}} at {{company}}（{{department}}）。\n{{bio}}\n\n翻譯方向：{{direction}}\n領域：{{domain}}\n\n原文：\n{{source_text}}\n\n請進行專業翻譯，要求：\n1. 準確傳達原意\n2. 使用該領域的專業術語\n3. 符合目標語言的表達習慣\n4. 保持原文的語氣和風格\n5. 對不確定的術語標注說明',
    variables: [
      { key: 'direction', label: '翻譯方向', placeholder: '例：英文→繁體中文 / 中文→英文' },
      { key: 'domain', label: '領域', placeholder: '例：軟體開發 / 金融 / 醫療 / 法律' },
      { key: 'source_text', label: '原文', placeholder: '貼上需要翻譯的文字' },
    ],
  },

  // ── 問題解決 ──────────────────────────────────────────
  {
    name: '問題分析與解決',
    description: '使用結構化方法分析和解決問題',
    content:
      '我是 {{display_name}}，{{role}} at {{company}}（{{department}}）。\n{{bio}}\n\n問題描述：\n{{problem}}\n\n已嘗試的方法：{{attempted}}\n\n請用結構化方法幫我分析這個問題：\n1. 問題定義與範圍\n2. 根本原因分析（5 Whys 或魚骨圖）\n3. 可能的解決方案（至少 3 種）\n4. 各方案的優缺點比較\n5. 推薦方案與執行步驟\n6. 預防措施建議',
    variables: [
      { key: 'problem', label: '問題描述', placeholder: '詳細描述遇到的問題' },
      { key: 'attempted', label: '已嘗試的方法', placeholder: '描述已經嘗試但未解決的方法' },
    ],
  },
]
