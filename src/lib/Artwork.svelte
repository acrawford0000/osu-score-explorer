<script lang="ts">
  import { onMount } from 'svelte';
  import { artwork, isDesktop } from './backend';
  let { id, available, hue = 240 }: { id: string; available: boolean; hue?: number } = $props();
  let element: HTMLDivElement;
  let image = $state<string | null>(null);
  onMount(() => {
    let alive = true;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          observer.disconnect();
          if (available && isDesktop)
            artwork(id)
              .then((src) => {
                if (alive) image = src;
              })
              .catch(() => {});
        }
      },
      { rootMargin: '100px' },
    );
    observer.observe(element);
    return () => {
      alive = false;
      observer.disconnect();
      image = null;
    };
  });
</script>

<div
  bind:this={element}
  class="artwork"
  class:has-image={image}
  style={`--hue:${hue};${image ? `background-image:url('${image}')` : ''}`}
  aria-hidden="true"
>
  <span class="art-orbit"></span>
</div>
