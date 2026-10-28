// Plain-language explanations behind every "What does this mean?" button.
// Each has: what it is, why it matters, and an everyday comparison.

export interface Explanation {
  title: string
  what: string
  why: string
  like?: string
}

export const EXPLAIN = {
  tokens: {
    title: 'Tokens',
    what: 'A token is a small piece of text, about three quarters of an English word. AI models read and write in tokens, and providers charge by the token.',
    why: 'Tokens are the unit everything else is measured in: cost, limits and speed.',
    like: '1,000 tokens is roughly 750 words, about one and a half printed pages.',
  },
  cost: {
    title: 'API-equivalent cost',
    what: "What these calls would cost at each provider's public price list (the API price).",
    why: 'On a subscription such as Claude Pro/Max or ChatGPT Plus/Pro you do not pay this per call. It shows how much value your plan gave you. A real API bill can differ because of discounts, free tiers and batch pricing.',
    like: "Like the price tag on a buffet dish: you paid a flat fee, but it's useful to know what you ate.",
  },
  calls: {
    title: 'Model calls',
    what: 'Each time an app asked an AI model for a reply.',
    why: 'One message from you can start many calls, because coding agents read files, run commands and check their work, each time asking the model again.',
  },
  savings: {
    title: 'Saved by the cache',
    what: 'The money the cache saved: re-read text priced as if it were new, minus what it actually cost.',
    why: 'Long conversations re-send the whole history on every call. The cache makes that repetition cheap.',
    like: 'Like re-reading your own notes instead of the whole textbook.',
  },
  input: {
    title: 'Fresh input',
    what: 'New text the model read for the first time: your message, files it opened, results from tools.',
    why: 'Priced at the normal input rate.',
  },
  cache_read: {
    title: 'Cache reads',
    what: 'Text the model had already seen and re-read from its short-term memory (the cache).',
    why: 'Usually about ten times cheaper than fresh input. It is most of the tokens in a long session, which is why big token numbers do not always mean big costs.',
    like: 'Re-reading a bookmarked page instead of searching the whole book again.',
  },
  cache_write: {
    title: 'Cache writes',
    what: 'Text saved into the cache so that later calls can re-read it cheaply.',
    why: 'It costs a little more than fresh input once (1.25 times, or 2 times for an hour-long cache), then saves money on every re-read.',
  },
  output: {
    title: 'Output',
    what: "Text the model wrote back, including any hidden 'thinking' it did before answering.",
    why: 'The priciest kind of token: usually about five times the price of fresh input.',
  },
  reasoning: {
    title: 'Thinking tokens',
    what: 'Tokens the model spends working a problem through before it answers. They are part of output.',
    why: 'You pay for them as output, even when the app hides them from you.',
  },
  daily: {
    title: 'Day by day',
    what: 'Each column is one day. The coloured pieces show which company served the models that did the work.',
    why: 'Spot busy days, and see how your usage is split between providers.',
    like: 'Hover or tap a column for exact numbers. Use "Show as table" for the full list.',
  },
  mix: {
    title: 'What kind of tokens',
    what: 'All tokens split into the four kinds providers charge for, from cheapest (lightest) to priciest (darkest).',
    why: 'Most tokens in coding sessions are cheap cache reads. The small output slice often costs the most.',
  },
  models: {
    title: 'Models',
    what: 'Which AI models did the work.',
    why: 'Bigger, smarter models cost more per token. Moving routine work to a smaller model is the quickest way to cut cost.',
  },
  projects: {
    title: 'Projects',
    what: 'The folder you were working in when the AI ran, usually one per project.',
    why: 'Shows which work used the most AI.',
  },
  sources: {
    title: 'Apps',
    what: "Which app on this computer made the calls. Gaṇanā reads each app's own usage log.",
    why: 'It reads only token counts, model names and times. It never reads or stores your conversations.',
  },
  unpriced: {
    title: 'Models without a price',
    what: "Some model names are not in the public price list. Usually that's a router or a nickname that hides which real model ran.",
    why: 'Their tokens are counted, but their cost is left out of every total until you say which real model each one is. That then prices all past and future calls.',
  },
} satisfies Record<string, Explanation>

export type ExplainKey = keyof typeof EXPLAIN
