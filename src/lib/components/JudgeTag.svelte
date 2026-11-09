<script lang="ts">
  // 自動の判断 (LockWatch の assess --new。design.md §7.4) の札。押せない状態の表示なので、ボタンとは形で分ける (角の丸い平らな札)。
  // 「前回の判断を使った」ものは Claude が見ていないので、枠を点線にする
  import type { AutoJudgment } from "$lib/types";
  import { URGENCY_TONE, URGENCY_WORD, WHERE_WORD, judgmentTitle } from "$lib/vulns";

  let { judgment }: { judgment: AutoJudgment } = $props();
</script>

<span class="judge {URGENCY_TONE[judgment.urgency] ?? 'low'}" class:reused={judgment.source === "reused"} title={judgmentTitle(judgment)}>
  {URGENCY_WORD[judgment.urgency] ?? judgment.urgency}{#if judgment.where}<span class="where">・{WHERE_WORD[judgment.where] ?? judgment.where}</span>{/if}
</span>

<style>
  .judge {
    font-size: 11px;
    border: 1px solid var(--tone);
    color: var(--tone);
    border-radius: 999px;
    padding: 0 8px;
    margin-left: 4px;
    white-space: nowrap;
    cursor: help;
  }

  .judge.reused {
    border-style: dashed;
  }

  .judge.high {
    --tone: var(--st-high);
    font-weight: 600;
  }

  .judge.mid {
    --tone: var(--st-mid);
  }

  .judge.low {
    --tone: var(--st-low);
  }

  .where {
    color: var(--ink-2);
    font-weight: normal;
  }
</style>
