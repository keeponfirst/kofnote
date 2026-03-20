// ============================================================
// POST /api/ai/classify
// ============================================================
// Classifies text into a record type + generates title
// ============================================================

import { callAI } from '../ai/router.js';
import { validateTextInput } from '../utils.js';

const SYSTEM_PROMPT = `You are a record classifier for a personal knowledge management system.
Given user input text, classify it into one of these types and generate a concise title.

Record types:
- decision: Choices, trade-offs, rationale for a decision made
- idea: Inspirations, possibilities, creative thoughts
- backlog: Future tasks, TODOs, things to do later
- worklog: Daily activities, learnings, progress updates
- note: General notes that don't fit other categories

Respond in JSON format:
{
  "record_type": "decision|idea|backlog|worklog|note",
  "title": "Concise title in the same language as the input",
  "key_insight": "One-sentence summary of the core insight (optional)",
  "tags": ["tag1", "tag2"],
  "confidence": 0.0-1.0
}

Rules:
- Title should be concise (under 50 characters when possible)
- Use the SAME LANGUAGE as the input text
- Tags should be lowercase, 2-4 relevant keywords
- If uncertain, default to "note"`;

export async function handleClassify(body, user, env) {
  const { text, url } = body;

  if (!text && !url) {
    throw new Error('Missing required field: text or url');
  }
  validateTextInput(text);

  const userMessage = url
    ? `Classify this shared content:\nURL: ${url}\nText: ${text || '(no text provided)'}`
    : `Classify this text:\n${text}`;

  const result = await callAI(SYSTEM_PROMPT, userMessage, user, env);

  let data;
  try {
    data = JSON.parse(result.text);
  } catch {
    // If AI didn't return valid JSON, wrap it
    data = {
      record_type: 'note',
      title: text?.slice(0, 50) || 'Untitled',
      tags: [],
      confidence: 0.5,
    };
  }

  return {
    data,
    model: result.model,
    tokens: result.tokens,
  };
}
