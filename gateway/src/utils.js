// ============================================================
// Shared utilities
// ============================================================

/** Returns today's date string in YYYY-MM-DD format (UTC) */
export function todayKey() {
  return new Date().toISOString().slice(0, 10);
}

/** Validate text input length, throws on violation */
export function validateTextInput(text, maxLength = 10000) {
  if (text && text.length > maxLength) {
    throw new Error(`Input too long (max ${maxLength} characters)`);
  }
}
