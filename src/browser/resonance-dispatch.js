/**
 * resonance-dispatch.js — Natural-language → MCP tool mapper
 *
 * Detects intent from address-bar input and maps it to the appropriate MCP
 * tool, replacing the proprietary slash-command syntax with natural language.
 *
 * Detection heuristics (all case-insensitive, order matters):
 *
 *   Question intent  → browser_research
 *     - Ends with "?"
 *     - Starts with interrogative word (what, how, why, when, who, where,
 *       explain, tell me, find, search)
 *
 *   Debug intent     → page_debug
 *     - Contains "error", "broken", "bug", "not working", "failed",
 *       "debug", "inspect", "why is … not"
 *
 *   Summarise intent → page_summarise
 *     - Starts with "summarise", "summarize", "tldr", "summary",
 *       "what does this page", "explain this"
 *
 *   Pricing intent   → pricing_lookup
 *     - Contains "price", "cost", "how much", "pricing", "plan",
 *       "subscription", "compare plans"
 *
 * If no intent matches, or if the input looks like a URL or a short
 * keyword (< 4 words with no sentence markers), the dispatcher returns
 * null and the address bar treats the input as a normal navigation/search.
 */

'use strict';

/**
 * @typedef {'browser_research'|'page_debug'|'page_summarise'|'pricing_lookup'|null} ResonanceTool
 */

const QUESTION_STARTERS = [
  'what ', 'how ', 'why ', 'when ', 'who ', 'where ',
  'explain ', 'tell me', 'find ', 'search ',
];

const DEBUG_SIGNALS = [
  'error', 'broken', 'bug ', 'not working', 'failed',
  'debug', 'inspect', "why is", "doesn't work", "wont load",
  "won't load", "not loading", "404", "500",
];

const SUMMARISE_STARTERS = [
  'summarise', 'summarize', 'tldr', 'summary of',
  'what does this page', 'explain this', 'give me the gist',
];

const PRICING_SIGNALS = [
  'price', 'cost', 'how much', 'pricing', 'subscription',
  'compare plans', ' plan ', 'per month', 'per year',
  'annual fee', 'cheapest', 'most expensive',
];

/**
 * Detect which MCP tool (if any) should handle a given address-bar input.
 *
 * @param {string} raw - Raw text entered in the address bar
 * @returns {{ tool: ResonanceTool, args: object }|null}
 *   Returns null when the input should be treated as a URL or search query.
 */
export function detectResonanceIntent(raw) {
  const text = raw.trim();
  if (!text || text.length < 8) return null;

  // Looks like a URL → not a Resonance intent
  if (/^https?:\/\//i.test(text) || /^[a-z0-9-]+\.[a-z]{2,}/i.test(text)) {
    return null;
  }
  // Legacy slash-commands — map to tool equivalents and strip the prefix
  const legacyMap = {
    '/scholar': 'browser_research',
    '/oracle':  'pricing_lookup',
    '/scribe':  'page_summarise',
    '/debug':   'page_debug',
  };
  for (const [cmd, tool] of Object.entries(legacyMap)) {
    if (text.toLowerCase().startsWith(cmd)) {
      const remainder = text.slice(cmd.length).trim();
      return { tool, args: buildArgs(tool, remainder || text), legacy: true };
    }
  }

  const lower = text.toLowerCase();

  // Debug intent (check before question — "why is X not working?" is debug)
  if (DEBUG_SIGNALS.some(s => lower.includes(s))) {
    return { tool: 'page_debug', args: { note: text }, legacy: false };
  }

  // Summarise intent
  if (SUMMARISE_STARTERS.some(s => lower.startsWith(s))) {
    return { tool: 'page_summarise', args: { format: 'structured' }, legacy: false };
  }

  // Pricing intent
  if (PRICING_SIGNALS.some(s => lower.includes(s))) {
    return { tool: 'pricing_lookup', args: { product_query: text }, legacy: false };
  }

  // Question intent (ends with "?" or starts with interrogative)
  const isQuestion = text.endsWith('?') ||
    QUESTION_STARTERS.some(s => lower.startsWith(s));
  if (isQuestion) {
    return { tool: 'browser_research', args: { question: text }, legacy: false };
  }

  return null;
}

/**
 * Build the MCP tool arguments object for a given tool and user query.
 * @param {ResonanceTool} tool
 * @param {string} query
 * @returns {object}
 */
function buildArgs(tool, query) {
  switch (tool) {
    case 'browser_research': return { question: query, include_page_context: true };
    case 'page_debug':       return { include_console: true, include_network: true };
    case 'page_summarise':   return { format: 'structured' };
    case 'pricing_lookup':   return { product_query: query, include_history: true };
    default:                 return {};
  }
}

/**
 * Invoke a Resonance MCP tool via the local MCP server.
 * Returns the tool result as a plain string suitable for the AI overlay.
 *
 * @param {ResonanceTool} tool
 * @param {object} args
 * @returns {Promise<string>}
 */
export async function invokeResonanceTool(tool, args) {
  const token = await getLocalMcpToken();
  const resp = await fetch(`http://127.0.0.1:39012`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'Authorization': `Bearer ${token}`,
    },
    body: JSON.stringify({
      jsonrpc: '2.0',
      id: Date.now(),
      method: 'tools/call',
      params: { name: tool, arguments: args },
    }),
  });
  if (!resp.ok) throw new Error(`MCP server error: ${resp.status}`);
  const data = await resp.json();
  if (data.error) throw new Error(data.error.message);
  const content = data.result?.content;
  if (Array.isArray(content) && content[0]?.text) return content[0].text;
  return JSON.stringify(data.result ?? data);
}

/** Read the local MCP session token from Tauri IPC. */
async function getLocalMcpToken() {
  const invoke = window.__TAURI__?.invoke;
  if (!invoke) return '';
  try {
    return (await invoke('cmd_mcp_session_token')) ?? '';
  } catch {
    return '';
  }
}
