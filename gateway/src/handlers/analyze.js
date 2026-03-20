// ============================================================
// POST /api/ai/analyze
// ============================================================
// Analyzes decisions/ideas with pros, cons, and recommendations
// ============================================================

import { callAI } from '../ai/router.js';
import { validateTextInput } from '../utils.js';

const SYSTEM_PROMPT = `You are an analytical assistant for a personal knowledge management system.
Given a decision or idea, provide a structured analysis with pros, cons, and a recommendation.

Respond in JSON format:
{
  "analysis": {
    "pros": ["pro 1", "pro 2"],
    "cons": ["con 1", "con 2"],
    "risks": ["risk 1"],
    "recommendation": "Your recommendation based on the analysis"
  },
  "summary": "Brief 1-2 sentence overview of the analysis",
  "tags": ["tag1", "tag2"]
}

Rules:
- Be objective and balanced
- Provide 2-4 pros and cons each
- Risks should highlight potential pitfalls
- Recommendation should be actionable
- Use the SAME LANGUAGE as the input text`;

export async function handleAnalyze(body, user, env) {
  const { text, title, context } = body;

  if (!text) {
    throw new Error('Missing required field: text');
  }
  validateTextInput(text);

  let userMessage = '';
  if (title) userMessage += `Title: ${title}\n\n`;
  if (context) userMessage += `Context: ${context}\n\n`;
  userMessage += `Analyze this:\n${text}`;

  const result = await callAI(SYSTEM_PROMPT, userMessage, user, env);

  let data;
  try {
    data = JSON.parse(result.text);
  } catch {
    data = {
      analysis: {
        pros: [],
        cons: [],
        risks: [],
        recommendation: result.text,
      },
      summary: '',
      tags: [],
    };
  }

  return {
    data,
    model: result.model,
    tokens: result.tokens,
  };
}
