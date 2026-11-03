<script setup lang="ts">
// The repository's front page (DESIGN v2, canvas board 01).
//
// It answers, in this order, the questions every visit starts with: which
// branch am I on, where does it stand against its upstream, and what does that
// state ask me to do. The answer is a sentence and a drawing rather than a pair
// of arrows in a corner — the arrows were always there, and nobody read them.
//
// One state, one primary action. A diverged branch offers "pull and rebase",
// not "push": the push would be refused, and a button that leads to a refusal
// is worse than no button.
//
// The counts are only as true as the last fetch, so the card always says when
// that was, and says so louder once it is old.

import { computed, onBeforeUnmount, ref } from "vue";
import {
  app,
  changedCount,
  createBranch,
  fetchRemote,
  goToZone,
  pullRemote,
  pullWith,
  pushBranch,
  showScreen,
} from "../state";
import { ago, laneOf, statusBadge, when } from "../format";
import { count, t } from "../i18n";
import type { BranchRow, LastCommit } from "../ipc";

/// A fetch older than this and the card says its numbers may be stale.
const STALE_AFTER = 24 * 3600;
/// A branch whose tip has not moved in this long reads as idle.
const IDLE_AFTER = 60 * 86_400;
/// How many branches the table lists before it stops being a summary.
const BRANCHES_SHOWN = 6;

type Kind =
  | "operation"
  | "detached"
  | "unborn"
  | "local"
  | "gone"
  | "diverged"
  | "ahead"
  | "behind"
  | "synced";

const summary = computed(() => app.summary);
const tracking = computed(() => summary.value?.tracking ?? null);
const branch = computed(() => summary.value?.head ?? "");
const upstream = computed(() => tracking.value?.upstream ?? "");
const remote = computed(() => upstream.value.split("/")[0] || "origin");
const ahead = computed(() => tracking.value?.ahead ?? 0);
const behind = computed(() => tracking.value?.behind ?? 0);
const busy = computed(() => app.running !== null || !!app.gitUnusable);

const kind = computed<Kind>(() => {
  const s = summary.value;
  if (!s) return "synced";
  if (s.operation) return "operation";
  if (s.head_kind === "detached") return "detached";
  if (s.head_kind === "unborn") return "unborn";
  const tr = s.tracking;
  if (!tr) return "local";
  if (tr.gone) return "gone";
  if (tr.ahead > 0 && tr.behind > 0) return "diverged";
  if (tr.ahead > 0) return "ahead";
  if (tr.behind > 0) return "behind";
  return "synced";
});

/// The pill's colour: direction where there is one, warning where something
/// needs attention, quiet otherwise.
const tone = computed(() => {
  switch (kind.value) {
    case "synced":
      return "success";
    case "ahead":
      return "ahead";
    case "behind":
      return "behind";
    case "operation":
      return "danger";
    case "diverged":
    case "detached":
    case "gone":
      return "warning";
    default:
      return "muted";
  }
});

const commitShort = computed(() => summary.value?.last_commit?.id.short ?? "");

const title = computed(() => {
  const params = { upstream: upstream.value, branch: branch.value };
  switch (kind.value) {
    case "ahead":
      return count("overview.ahead.title", ahead.value);
    case "behind":
      return count("overview.behind.title", behind.value);
    case "diverged":
      return t("overview.diverged.title", { ahead: ahead.value, behind: behind.value });
    case "operation":
      return t("overview.operation.title", { operation: summary.value?.operation ?? "" });
    default:
      return t(`overview.${kind.value}.title`, params);
  }
});

const detail = computed(() =>
  t(`overview.${kind.value}.detail`, {
    upstream: upstream.value,
    branch: branch.value,
    commit: commitShort.value,
  }),
);

// ── When the remote was last asked ──────────────────────────────────────────

const now = ref(Math.floor(Date.now() / 1000));
// The "il y a 4 min" has to move on its own; a minute is its resolution.
const ticking = setInterval(() => (now.value = Math.floor(Date.now() / 1000)), 60_000);
onBeforeUnmount(() => clearInterval(ticking));

const fetchedAt = computed(() => summary.value?.fetched_at ?? null);
const stale = computed(() => fetchedAt.value === null || now.value - fetchedAt.value > STALE_AFTER);
const fetchedLine = computed(() =>
  fetchedAt.value === null
    ? t("overview.neverFetched")
    : t("overview.fetched", { ago: ago(fetchedAt.value, now.value * 1000) }),
);

// ── The drawing ─────────────────────────────────────────────────────────────

/// At most five dots a side; past that the line carries a "+n".
const DOTS = 5;
const outgoingDots = computed(() => Math.min(ahead.value, DOTS));
const incomingDots = computed(() => Math.min(behind.value, DOTS));
const hasRemoteSide = computed(() =>
  ["synced", "ahead", "behind", "diverged"].includes(kind.value),
);

function dotX(index: number, total: number, from: number, to: number): number {
  if (total <= 1) return to;
  return from + ((to - from) * index) / (total - 1);
}

// ── The commit lists ────────────────────────────────────────────────────────

const outgoing = computed<readonly LastCommit[]>(() => app.divergence?.outgoing ?? []);
const incoming = computed<readonly LastCommit[]>(() => app.divergence?.incoming ?? []);

// ── Detached: a name for the branch that keeps the commit ──────────────────

const naming = ref("");
function keepHere(): void {
  const name = naming.value.trim();
  if (!name) return;
  createBranch(name, "", true);
  naming.value = "";
}

// ── The cards ───────────────────────────────────────────────────────────────

const changes = computed(() => (app.open ? changedCount() : 0));
const counts = computed(() => summary.value?.counts);
const conflicts = computed(() => counts.value?.conflicted ?? 0);
const untracked = computed(() => counts.value?.untracked ?? 0);
const stashes = computed(() => summary.value?.stashes ?? 0);
const last = computed(() => summary.value?.last_commit ?? null);

/// The working copy's rows, the first few, for the card.
const files = computed(() => {
  const status = app.status;
  if (status.status !== "ready") return [];
  return status.value.slice(0, 4);
});

// ── The branches ────────────────────────────────────────────────────────────

const branches = computed(() => {
  const refs = app.refs;
  if (refs.status !== "ready") return [];
  const rows = [...refs.value.branches];
  // Where you are first, then what is still open, freshest first; merged ones
  // last — they are labels, not work.
  rows.sort((a, b) => {
    if (a.head !== b.head) return a.head ? -1 : 1;
    if (a.merged !== b.merged) return a.merged ? 1 : -1;
    return a.age - b.age;
  });
  return rows.slice(0, BRANCHES_SHOWN);
});
const hiddenBranches = computed(() =>
  app.refs.status === "ready" ? Math.max(0, app.refs.value.branches.length - BRANCHES_SHOWN) : 0,
);

function laneStyle(name: string): Record<string, string> {
  return { "--chip": `var(--lane-${laneOf(name)})` };
}

function branchStatus(row: BranchRow): { label: string; tone: string } {
  if (row.head) return { label: t("overview.branch.current"), tone: "accent" };
  if (row.merged) return { label: t("overview.branch.merged"), tone: "success" };
  if (row.age > IDLE_AFTER) return { label: t("overview.branch.idle"), tone: "muted" };
  return { label: t("overview.branch.active"), tone: "info" };
}
</script>

<template>
  <main class="overview" tabindex="0" data-zone="2" @focus="goToZone(2)">
    <template v-if="summary">
      <!-- The state card: a sentence, a drawing, an action. -->
      <section class="ov-card ov-state">
        <div class="ov-state-text">
          <span class="ov-label">{{ t("overview.state") }}</span>
          <div class="ov-chips">
            <span class="ov-chip mono" :style="laneStyle(branch)">
              {{ kind === "detached" ? commitShort : branch }}
            </span>
            <template v-if="tracking && !tracking.gone">
              <span class="ov-arrows">
                <span v-if="ahead > 0" class="ahead">↑{{ ahead }}</span>
                <span v-if="behind > 0" class="behind">↓{{ behind }}</span>
                <span v-if="ahead === 0 && behind === 0" class="synced">=</span>
              </span>
              <span class="ov-chip remote mono" :style="laneStyle(branch)">{{ upstream }}</span>
            </template>
            <span class="ov-pill" :class="tone">{{ t(`overview.pill.${kind}`) }}</span>
          </div>
          <h1 class="ov-title">{{ title }}</h1>
          <p class="ov-detail">{{ detail }}</p>

          <div class="ov-actions">
            <template v-if="kind === 'diverged'">
              <button class="primary" :disabled="busy" @click="pullWith('rebase')">
                {{ t("overview.pullRebase") }}
              </button>
              <button :disabled="busy" @click="pullWith('merge')">
                {{ t("overview.pullMerge") }}
              </button>
            </template>
            <button
              v-else-if="kind === 'ahead'"
              class="primary"
              :disabled="busy"
              @click="pushBranch(false)"
            >
              {{ t("overview.push", { n: ahead }) }}
            </button>
            <button
              v-else-if="kind === 'behind'"
              class="primary"
              :disabled="busy"
              @click="pullRemote()"
            >
              {{ t("overview.pull", { n: behind }) }}
            </button>
            <button
              v-else-if="kind === 'local' || kind === 'gone'"
              :class="{ primary: kind === 'local' }"
              :disabled="busy"
              @click="pushBranch(false)"
            >
              {{ t("overview.publish", { remote }) }}
            </button>
            <button
              v-else-if="kind === 'operation'"
              class="primary"
              @click="showScreen('working-copy')"
            >
              {{ t("overview.openWorkingCopy") }}
            </button>
            <form v-else-if="kind === 'detached'" class="ov-name" @submit.prevent="keepHere">
              <input
                v-model="naming"
                type="text"
                class="mono"
                :placeholder="t('overview.branchName')"
                :aria-label="t('overview.branchName')"
              />
              <button class="primary" type="submit" :disabled="!naming.trim()">
                {{ t("overview.createBranch") }}
              </button>
            </form>
          </div>

          <div class="ov-fetched" :class="{ stale }">
            <span class="ov-dot" />
            <span>{{ fetchedLine }}</span>
            <span v-if="stale && hasRemoteSide">· {{ t("overview.stale") }}</span>
            <span>·</span>
            <button class="link" :disabled="busy" @click="fetchRemote()">
              {{ t("overview.checkNow") }}
            </button>
          </div>
        </div>

        <div v-if="hasRemoteSide" class="ov-state-drawing">
          <svg viewBox="0 0 480 120" class="ov-svg" role="img" :aria-label="title">
            <path
              v-if="ahead > 0"
              :d="behind > 0 ? 'M24 60 C 60 60, 64 24, 100 24 L 452 24' : 'M24 60 L 452 60'"
              class="line-ahead"
            />
            <path
              v-if="behind > 0"
              :d="ahead > 0 ? 'M24 60 C 60 60, 64 96, 100 96 L 452 96' : 'M24 60 L 452 60'"
              class="line-behind"
            />
            <path v-if="ahead === 0 && behind === 0" d="M24 60 L 452 60" class="line-synced" />
            <circle cx="24" cy="60" r="7" class="node-base" />
            <circle
              v-for="i in outgoingDots"
              :key="`o${i}`"
              :cx="dotX(i - 1, outgoingDots, 150, 452)"
              :cy="behind > 0 ? 24 : 60"
              r="7"
              class="node-ahead"
            />
            <circle
              v-for="i in incomingDots"
              :key="`i${i}`"
              :cx="dotX(i - 1, incomingDots, 150, 452)"
              :cy="ahead > 0 ? 96 : 60"
              r="7"
              class="node-behind"
            />
            <circle v-if="ahead === 0 && behind === 0" cx="452" cy="60" r="9" class="node-synced" />
          </svg>

          <div class="ov-lists">
            <div v-if="ahead > 0" class="ov-list">
              <span class="ov-label ahead">{{ t("overview.outgoing", { n: ahead }) }}</span>
              <div v-for="c in outgoing" :key="c.id.full" class="ov-commit">
                <span class="mono ov-hash">{{ c.id.short }}</span>
                <span class="ov-subject">{{ c.summary }}</span>
              </div>
              <span v-if="ahead > outgoing.length && outgoing.length > 0" class="ov-more">
                {{ t("overview.more", { n: ahead - outgoing.length }) }}
              </span>
            </div>
            <div v-if="behind > 0" class="ov-list">
              <span class="ov-label behind">{{ t("overview.incoming", { n: behind }) }}</span>
              <div v-for="c in incoming" :key="c.id.full" class="ov-commit">
                <span class="mono ov-hash">{{ c.id.short }}</span>
                <span class="ov-subject">{{ c.summary }}</span>
                <span class="ov-who">{{ c.author }} · {{ ago(c.when) }}</span>
              </div>
              <span v-if="behind > incoming.length && incoming.length > 0" class="ov-more">
                {{ t("overview.more", { n: behind - incoming.length }) }}
              </span>
            </div>
          </div>
        </div>
      </section>

      <!-- Three cards: what is changing, what is set aside, what was last done. -->
      <div class="ov-grid">
        <section class="ov-card ov-small">
          <header class="ov-small-head">
            <span class="ov-label">{{ t("overview.workingCopy") }}</span>
            <button class="link" @click="showScreen('working-copy')">
              {{ t("overview.open") }}
            </button>
          </header>
          <div class="ov-figure">
            <span class="ov-number">{{ changes }}</span>
            <span class="ov-muted">
              {{ changes > 0 ? count("overview.changes", changes) : t("overview.clean") }}
            </span>
          </div>
          <span v-if="conflicts > 0" class="ov-pill danger">{{
            count("overview.conflicts", conflicts)
          }}</span>
          <div class="ov-files">
            <div v-for="row in files" :key="row.path" class="ov-file">
              <span class="badge" :class="statusBadge(row).kind">{{
                statusBadge(row).letter
              }}</span>
              <span class="mono ov-path">{{ row.path }}</span>
            </div>
            <span v-if="untracked > 0" class="ov-more">{{
              t("overview.untracked", { n: untracked })
            }}</span>
          </div>
        </section>

        <section class="ov-card ov-small">
          <header class="ov-small-head">
            <span class="ov-label">{{ t("overview.stashes") }}</span>
            <button v-if="stashes > 0" class="link" @click="showScreen('stashes')">
              {{ t("overview.open") }}
            </button>
          </header>
          <div class="ov-figure">
            <span class="ov-number">{{ stashes }}</span>
            <span class="ov-muted">
              {{ stashes > 0 ? count("overview.stashCount", stashes) : t("overview.noStash") }}
            </span>
          </div>
        </section>

        <section class="ov-card ov-small">
          <header class="ov-small-head">
            <span class="ov-label">{{ t("overview.lastCommit") }}</span>
            <button class="link" @click="showScreen('history')">{{ t("overview.open") }}</button>
          </header>
          <template v-if="last">
            <span class="ov-last-subject">{{ last.summary }}</span>
            <span class="ov-muted">
              <span class="mono">{{ last.id.short }}</span> · {{ last.author }} ·
              {{ when(last.when) }}
            </span>
          </template>
          <span v-else class="ov-muted">—</span>
        </section>
      </div>

      <!-- The branches that are still somebody's work. -->
      <section class="ov-card ov-branches">
        <header class="ov-branches-head">
          <span class="ov-section">{{ t("overview.branches") }}</span>
          <span v-if="hiddenBranches > 0" class="ov-muted">+{{ hiddenBranches }}</span>
        </header>
        <table v-if="branches.length > 0" class="ov-table">
          <thead>
            <tr>
              <th>{{ t("overview.col.branch") }}</th>
              <th>{{ t("overview.col.remote") }}</th>
              <th>{{ t("overview.col.last") }}</th>
              <th class="right">{{ t("overview.col.status") }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="row in branches" :key="row.name">
              <td>
                <span class="ov-chip mono" :style="laneStyle(row.name)">{{ row.name }}</span>
              </td>
              <td>
                <template v-if="!row.tracking">
                  <span class="ov-muted">{{ t("overview.branch.local") }}</span>
                </template>
                <template v-else-if="row.tracking.gone">
                  <span class="warning">{{ t("overview.branch.gone") }}</span>
                </template>
                <template v-else-if="row.tracking.ahead === 0 && row.tracking.behind === 0">
                  <span class="synced">✓</span>
                  <span class="ov-muted">{{ t("overview.branch.synced") }}</span>
                </template>
                <template v-else>
                  <span v-if="row.tracking.ahead > 0" class="ahead">↑{{ row.tracking.ahead }}</span>
                  <span v-if="row.tracking.behind > 0" class="behind"
                    >↓{{ row.tracking.behind }}</span
                  >
                  <span class="ov-muted mono">{{ row.tracking.upstream }}</span>
                </template>
              </td>
              <td class="ov-muted">{{ ago(now - row.age, now * 1000) }}</td>
              <td class="right">
                <span class="ov-pill" :class="branchStatus(row).tone">{{
                  branchStatus(row).label
                }}</span>
              </td>
            </tr>
          </tbody>
        </table>
        <p v-else class="ov-muted ov-empty">{{ t("overview.noBranches") }}</p>
      </section>
    </template>
  </main>
</template>
