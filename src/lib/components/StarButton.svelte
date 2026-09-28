<script lang="ts">
  import type { Project } from "$lib/derive";
  import { errorText, setStarred, toast } from "$lib/store.svelte";

  let { project, big = false }: { project: Project; big?: boolean } = $props();

  async function toggle(e: MouseEvent) {
    // 行の選択まで伝えない
    e.stopPropagation();
    try {
      await setStarred(project.prefKey, !project.starred);
    } catch (err) {
      toast(errorText(err));
    }
  }
</script>

<!-- 絵文字ではなく記号の文字にして、テーマに合わせて色を変える -->
<button
  class="star"
  class:on={project.starred}
  class:big
  onclick={toggle}
  title={project.starred ? "スターを外す" : "スターを付ける"}
  aria-label={project.starred ? `${project.name} のスターを外す` : `${project.name} にスターを付ける`}
  aria-pressed={project.starred}>{project.starred ? "★" : "☆"}</button
>

<style>
  .star {
    border: none;
    background: transparent;
    padding: 0 2px;
    font-size: 15px;
    line-height: 1;
    color: var(--muted);
    border-radius: 4px;
  }

  .star.big {
    font-size: 18px;
  }

  .star.on {
    color: var(--star);
  }

  .star:hover:not(:disabled) {
    background: var(--hover);
    color: var(--star);
  }
</style>
