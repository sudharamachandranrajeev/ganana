<script lang="ts">
  import { Tween } from 'svelte/motion'
  import { tweenOptions } from './motion'

  let { value, format }: { value: number; format: (n: number) => string } = $props()

  const shown = Tween.of(() => value, tweenOptions(900))
</script>

<span aria-label={format(value)}>{format(shown.current)}</span>
