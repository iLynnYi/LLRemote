<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { NModal } from "naive-ui";
import { ChevronForwardOutline } from "@vicons/ionicons5";
import { useI18n } from "vue-i18n";
import DistributionMarkdownInline from "./DistributionMarkdownInline.vue";

const props = defineProps<{ active: boolean }>();
const { t } = useI18n();
const documents = {
  install: () => import("../../../../docs/distribution/INSTALL.md?raw"),
  privacy: () => import("../../../../docs/distribution/PRIVACY.md?raw"),
};
type DocumentId = keyof typeof documents;
type MarkdownBlock = { kind: "heading" | "paragraph" | "list" | "quote"; level?: number; ordered?: boolean; lines: string[] };
const documentIds = Object.keys(documents) as DocumentId[];
const selected = ref<DocumentId | null>(null);
const documentText = ref("");
const failed = ref(false);
const loading = ref(false);
const readerBody = ref<HTMLElement | null>(null);
let generation = 0;
const title = computed(() => selected.value ? t(`settings.about.distribution.documents.${selected.value}`) : "");
const blocks = computed(() => {
  const parsed = parseMarkdown(documentText.value);
  return parsed[0]?.kind === "heading" && parsed[0].level === 1 ? parsed.slice(1) : parsed;
});

// The bundled Markdown uses headings, simple lists, quotations, and inline formatting.
// Render text through Vue interpolation rather than interpreting document HTML.
function parseMarkdown(source: string): MarkdownBlock[] {
  const result: MarkdownBlock[] = [];
  let consecutive = false;
  for (const line of source.split(/\r?\n/)) {
    if (!line.trim()) {
      consecutive = false;
      continue;
    }
    const heading = /^(#{1,6})\s+(.+)$/.exec(line);
    if (heading) {
      result.push({ kind: "heading", level: heading[1].length, lines: [heading[2]] });
    } else {
      const list = /^([-*]|\d+\.)\s+(.+)$/.exec(line);
      const quote = /^>\s?(.+)$/.exec(line);
      const previous = result[result.length - 1];
      if (list) {
        const ordered = /\d/.test(list[1]);
        if (consecutive && previous?.kind === "list" && previous.ordered === ordered) previous.lines.push(list[2]);
        else result.push({ kind: "list", ordered, lines: [list[2]] });
      } else if (quote) {
        if (consecutive && previous?.kind === "quote") previous.lines.push(quote[1]);
        else result.push({ kind: "quote", lines: [quote[1]] });
      } else if (consecutive && previous?.kind === "paragraph") {
        previous.lines.push(line);
      } else {
        result.push({ kind: "paragraph", lines: [line] });
      }
    }
    consecutive = true;
  }
  return result;
}

function openDocument(id: DocumentId) {
  selected.value = id;
}

function navigateToHeading(href: string) {
  const target = Array.from(readerBody.value?.querySelectorAll("h1, h2") ?? [])
    .find(heading => heading.textContent?.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") === href.slice(1));
  target?.scrollIntoView({ block: "start" });
}

watch(() => props.active, active => {
  if (!active) selected.value = null;
});

watch(selected, async id => {
  const attempt = ++generation;
  documentText.value = "";
  failed.value = false;
  if (!id) {
    loading.value = false;
    return;
  }
  loading.value = true;
  try {
    const document = await documents[id]();
    if (attempt === generation) documentText.value = document.default;
  } catch {
    if (attempt === generation) failed.value = true;
  } finally {
    if (attempt === generation) loading.value = false;
  }
});
</script>

<template>
  <section class="distribution-information" :aria-label="t('settings.about.distribution.title')">
    <h3>{{ t("settings.about.distribution.title") }}</h3>
    <div class="distribution-links">
      <button v-for="id in documentIds" :key="id" type="button" class="distribution-link" @click="openDocument(id)">
        <span>{{ t(`settings.about.distribution.documents.${id}`) }}</span>
        <ChevronForwardOutline aria-hidden="true" />
      </button>
    </div>
  </section>

  <NModal
    :show="selected !== null && active"
    preset="card"
    class="distribution-reader"
    :title="title"
    :style="{ width: 'min(820px, calc(100vw - 32px))' }"
    :content-style="{ padding: '0' }"
    @update:show="show => { if (!show) selected = null; }"
  >
    <div ref="readerBody" class="distribution-reader-body" :aria-label="title" tabindex="0">
      <p v-if="loading" role="status">{{ t("settings.about.distribution.loading") }}</p>
      <p v-else-if="failed" role="alert">{{ t("settings.about.distribution.failed") }}</p>
      <article v-else class="distribution-markdown">
        <template v-for="(block, index) in blocks" :key="index">
          <h1 v-if="block.kind === 'heading' && block.level === 1">
            <DistributionMarkdownInline :source="block.lines[0]" @navigate="navigateToHeading" />
          </h1>
          <h2 v-else-if="block.kind === 'heading'">
            <DistributionMarkdownInline :source="block.lines[0]" @navigate="navigateToHeading" />
          </h2>
          <component :is="block.ordered ? 'ol' : 'ul'" v-else-if="block.kind === 'list'">
            <li v-for="(line, lineIndex) in block.lines" :key="lineIndex">
              <DistributionMarkdownInline :source="line" @navigate="navigateToHeading" />
            </li>
          </component>
          <blockquote v-else-if="block.kind === 'quote'">
            <DistributionMarkdownInline :source="block.lines.join(' ')" @navigate="navigateToHeading" />
          </blockquote>
          <p v-else><DistributionMarkdownInline :source="block.lines.join(' ')" @navigate="navigateToHeading" /></p>
        </template>
      </article>
    </div>
  </NModal>
</template>

<style scoped>
.distribution-information { border-top: 1px solid #edf0f4; padding: 18px 20px 20px; }
.distribution-information h3 { margin: 0 0 12px; color: #1f2933; font-size: 14px; font-weight: 600; }
.distribution-links { overflow: hidden; border: 1px solid #dde3ea; border-radius: 8px; }
.distribution-link { display: flex; align-items: center; justify-content: space-between; width: 100%; min-height: 48px; gap: 12px; padding: 10px 14px; border: 0; border-top: 1px solid #edf0f4; background: #fff; color: #24313f; font: inherit; text-align: left; cursor: pointer; }
.distribution-link:first-child { border-top: 0; }
.distribution-link:hover { background: #f7faf9; color: #146652; }
.distribution-link:focus-visible { position: relative; outline: 2px solid #1f8a70; outline-offset: -2px; }
.distribution-link svg { flex: none; width: 16px; height: 16px; color: #8995a2; }
.distribution-reader-body { max-height: min(72vh, 720px); overflow-y: auto; padding: 20px 24px 28px; color: #24313f; font-size: 14px; line-height: 1.7; overflow-wrap: anywhere; }
.distribution-reader-body:focus-visible { outline: 2px solid #1f8a70; outline-offset: -2px; }
.distribution-reader-body > p { margin: 0; }
.distribution-markdown > :first-child { margin-top: 0; }
.distribution-markdown > :last-child { margin-bottom: 0; }
.distribution-markdown h1 { margin: 0 0 16px; font-size: 20px; line-height: 1.4; }
.distribution-markdown h2 { margin: 24px 0 10px; font-size: 16px; line-height: 1.45; }
.distribution-markdown p { margin: 12px 0; }
.distribution-markdown ul, .distribution-markdown ol { margin: 10px 0 16px; padding-left: 22px; }
.distribution-markdown li + li { margin-top: 8px; }
.distribution-markdown blockquote { margin: 12px 0; padding: 6px 12px; border-left: 3px solid #9cbcb2; background: #f7faf9; }
.distribution-markdown :deep(code) { padding: 1px 4px; border-radius: 3px; background: #f0f3f5; font-size: 0.92em; }
.distribution-markdown :deep(a) { color: #146652; text-decoration: underline; }
@media (max-width: 600px) { .distribution-reader-body { padding: 16px; } }
</style>
