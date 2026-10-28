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
  limits: {
    title: 'Usage limits',
    what: 'Subscriptions such as Claude Pro/Max and ChatGPT Plus/Pro cap how much you can use in a window of time: a 5-hour window and a weekly one. The bar shows how much of each window is used, as the provider itself reports it.',
    why: 'When a window is full you have to wait for it to reset. Knowing where you stand lets you pace big jobs or switch to a lighter model in time.',
    like: 'Like a phone data allowance that refills on a schedule.',
  },
  history: {
    title: 'Window history',
    what: 'The line shows how the used percentage grew in the current window. The dashed line continues it at your recent pace, up to the time it would be full or the window resets.',
    why: 'A steep line means heavy use right now. If the dashed line reaches the top before the reset, you will run out at this pace.',
  },
  plans: {
    title: 'Your plans',
    what: 'The subscriptions you pay for, and what the usage they covered this billing period would have cost at public API prices.',
    why: 'It shows whether a plan is worth its price for how you work. Only usage Gaṇanā can see is counted: chats in the web or phone apps leave no logs on this computer.',
    like: "If the API price is far above what you pay, the plan is a bargain. If it's below, paying per use could be cheaper.",
  },
  alerts: {
    title: 'Alerts',
    what: 'A desktop notification when a usage window passes one of these levels, once per window.',
    why: 'You hear about a limit before you hit it, not when work stops. Alerts need Gaṇanā running; closing the window keeps it in the tray.',
  },
  connect: {
    title: 'Connect Claude Pro/Max',
    what: "Claude Code shares your plan's usage limits only with its status line, the line under its input box. Gaṇanā can be that status line: it keeps only the two limit numbers and shows them in Claude Code too.",
    why: 'Without it, Claude limits are not visible to any app. The change is one setting in Claude Code, backed up first, and undone with one click. A status line you already have keeps working.',
  },
} satisfies Record<string, Explanation>

export type ExplainKey = keyof typeof EXPLAIN
