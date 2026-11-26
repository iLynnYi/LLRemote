<script setup lang="ts">
import { computed } from "vue";

const props = defineProps<{ source: string }>();
const emit = defineEmits<{ navigate: [href: string] }>();
type Part = { kind: "text" | "code" | "strong" | "em" | "link"; text: string; href?: string };

const parts = computed(() => {
  const result: Part[] = [];
  const pattern = /(`[^`]+`|\*\*\*[^*]+\*\*\*|\*\*[^*]+\*\*|\*[^*]+\*|\[[^\]]+\]\([^)]+\)|<https?:\/\/[^>]+>)/g;
  let offset = 0;
  for (const match of props.source.matchAll(pattern)) {
    const start = match.index;
    if (start > offset) result.push({ kind: "text", text: props.source.slice(offset, start) });
    const token = match[0];
    if (token.startsWith("`")) result.push({ kind: "code", text: token.slice(1, -1) });
    else if (token.startsWith("***")) result.push({ kind: "strong", text: token.slice(3, -3) });
    else if (token.startsWith("**")) result.push({ kind: "strong", text: token.slice(2, -2) });
    else if (token.startsWith("*")) result.push({ kind: "em", text: token.slice(1, -1) });
    else if (token.startsWith("[")) {
      const link = /^\[([^\]]+)\]\(([^)]+)\)$/.exec(token);
      if (link) result.push({ kind: "link", text: link[1], href: link[2] });
    } else result.push({ kind: "link", text: token.slice(1, -1), href: token.slice(1, -1) });
    offset = start + token.length;
  }
  if (offset < props.source.length) result.push({ kind: "text", text: props.source.slice(offset) });
  return result;
});

function onLinkClick(event: MouseEvent, href: string | undefined) {
  if (!href?.startsWith("#")) return;
  event.preventDefault();
  emit("navigate", href);
}
</script>

<template>
  <template v-for="(part, index) in parts" :key="index">
    <code v-if="part.kind === 'code'">{{ part.text }}</code>
    <strong v-else-if="part.kind === 'strong'">{{ part.text }}</strong>
    <em v-else-if="part.kind === 'em'">{{ part.text }}</em>
    <a v-else-if="part.kind === 'link'" :href="part.href" target="_blank" rel="noopener noreferrer" @click="onLinkClick($event, part.href)">{{ part.text }}</a>
    <template v-else>{{ part.text }}</template>
  </template>
</template>
