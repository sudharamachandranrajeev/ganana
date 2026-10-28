import { cubicOut } from 'svelte/easing'

const query = typeof matchMedia === 'function' ? matchMedia('(prefers-reduced-motion: reduce)') : null

/** Tween options that collapse to an instant change for reduced motion. */
export const tweenOptions = (duration = 700) => ({ duration: query?.matches ? 0 : duration, easing: cubicOut })
