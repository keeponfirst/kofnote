// ============================================================
// AI Model Router
// ============================================================
// Routes requests to the correct AI model based on user tier
// ============================================================

const MODEL_CONFIG = {
  free: {
    provider: 'gemini',
    model: 'gemini-2.0-flash',
  },
  pro: {
    provider: 'anthropic',
    model: 'claude-haiku-4-5-20251001',
  },
  premium: {
    provider: 'anthropic',
    model: 'claude-sonnet-4-6',
  },
};

/**
 * Call the appropriate AI model based on user tier.
 * @param {string} systemPrompt
 * @param {string} userMessage
 * @param {object} user - { id, tier }
 * @param {object} env
 * @returns {{ text, model, tokens }}
 */
export async function callAI(systemPrompt, userMessage, user, env) {
  const config = MODEL_CONFIG[user.tier] || MODEL_CONFIG.free;

  if (config.provider === 'gemini') {
    return callGemini(config.model, systemPrompt, userMessage, env);
  } else {
    return callAnthropic(config.model, systemPrompt, userMessage, env);
  }
}

// ─── Gemini ───

async function callGemini(model, systemPrompt, userMessage, env) {
  const url = `https://generativelanguage.googleapis.com/v1beta/models/${model}:generateContent?key=${env.GEMINI_API_KEY}`;

  const res = await fetch(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      system_instruction: {
        parts: [{ text: systemPrompt }],
      },
      contents: [
        {
          role: 'user',
          parts: [{ text: userMessage }],
        },
      ],
      generationConfig: {
        temperature: 0.3,
        maxOutputTokens: 1024,
        responseMimeType: 'application/json',
      },
    }),
  });

  if (!res.ok) {
    const errText = await res.text();
    console.error(`Gemini error (${res.status}):`, errText);
    throw new Error(`Gemini API error: ${res.status}`);
  }

  const data = await res.json();
  const text =
    data.candidates?.[0]?.content?.parts?.[0]?.text || '';
  const tokens =
    (data.usageMetadata?.promptTokenCount || 0) +
    (data.usageMetadata?.candidatesTokenCount || 0);

  return { text, model, tokens };
}

// ─── Anthropic ───

async function callAnthropic(model, systemPrompt, userMessage, env) {
  const res = await fetch('https://api.anthropic.com/v1/messages', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'x-api-key': env.ANTHROPIC_API_KEY,
      'anthropic-version': '2023-06-01',
    },
    body: JSON.stringify({
      model,
      max_tokens: 1024,
      system: systemPrompt,
      messages: [
        { role: 'user', content: userMessage },
      ],
    }),
  });

  if (!res.ok) {
    const errText = await res.text();
    console.error(`Anthropic error (${res.status}):`, errText);
    throw new Error(`Anthropic API error: ${res.status}`);
  }

  const data = await res.json();
  const text = data.content?.[0]?.text || '';
  const tokens =
    (data.usage?.input_tokens || 0) + (data.usage?.output_tokens || 0);

  return { text, model, tokens };
}
