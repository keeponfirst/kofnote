// ============================================================
// POST /api/ai/summarize
// ============================================================
// Summarizes long text and extracts key insight
// ============================================================

import { callAI } from '../ai/router.js';
import { validateTextInput } from '../utils.js';

const SYSTEM_PROMPT = `You are a summarization assistant for a personal knowledge management system.
Given a piece of text, generate a concise summary and extract the key insight.

Respond in JSON format:
{
  "summary": "2-4 sentence summary of the content",
  "key_insight": "The single most important takeaway",
  "tags": ["tag1", "tag2", "tag3"]
}

Rules:
- Summary should be 2-4 sentences, capturing the essential information
- Key insight should be ONE sentence - the most actionable or important point
- Use the SAME LANGUAGE as the input text
- Tags should be lowercase, 2-5 relevant keywords`;

export async function handleSummarize(body, user, env) {
  const { text, title } = body;

  if (!text) {
    throw new Error('Missing required field: text');
  }
  validateTextInput(text, 20000); // summarize 允許較長輸入

  const userMessage = title
    ? `Title: ${title}\n\nSummarize this text:\n${text}`
    : `Summarize this text:\n${text}`;

  const result = await callAI(SYSTEM_PROMPT, userMessage, user, env);

  let data;
  try {
    data = JSON.parse(result.text);
  } catch {
    data = {
      summary: result.text,
      key_insight: '',
      tags: [],
    };
  }

  return {
    data,
    model: result.model,
    tokens: result.tokens,
  };
}
