"""Deep Monte-Carlo (DouZero-style) self-play learner for the mtgenv gym — the *model-free* contrast
arm to the model-based tree-search arms (LightZero / MuZero, runs 4.0–4.2).

**The learning rule (DMC).** No critic, no bootstrapping, no policy gradient. Collect full self-play
episodes with an ε-greedy actor over the *legal* actions; the regression target for **every**
``(state, action)`` visited is that game's **final ±1 return** from the acting seat's perspective
(undiscounted, γ=1 — a terminal-only Monte-Carlo return). Regress ``Q(s, a)`` to it with MSE. That's
the whole objective (DouZero, *Deep Monte-Carlo* — arXiv:2106.06135, simplified to a single shared
value net for a symmetric 2-player game). Reward shaping is intentionally **OFF**: DMC learns the raw
±1, which is also exactly what eval scores.

**The head is ACTION-AS-INPUT.** MTG's action head is a fixed ``Discrete(98)`` of *factored slots*
whose meaning is positional (see ``codec.rs``): ``HAND[i]``/``PERM[i]``/``STACK[i]`` slots point at a
specific entity ROW the policy already sees in the observation (hand / battlefield / stack), while
``COMMIT``/``PLAYER``/``MODE``/``COLOR``/``NUMBER``/``YES``/``NO`` are abstract slots. So we score
``Q(s, a)`` from the *content of the object the action points at* (its encoded entity row) rather than
from a bare slot index — the DouZero insight that a value should be read off the action's own features,
so it generalises across which slot a given card happens to occupy. Abstract slots fall back to a
learned per-slot embedding. (If the action space were an opaque index with no recoverable per-action
content this would degrade to a plain indexed head; here the codec's positional factoring makes the
entity content recoverable, so we build the real thing. See the module-level note in ``DMCNet``.)

Everything eval/metrics/logging goes through ``evalkit`` (``DMCPolicy`` is the thin ``Policy`` adapter);
this module owns only *collection + the DMC update*. The self-play regime is **mirror**: the current
net plays BOTH seats (ε-greedy), and transitions from both seats are labelled and stored — twice the
data per game and a symmetric target, which a single shared Q-net wants.
"""

from __future__ import annotations

import numpy as np
import torch
import torch.nn as nn

from .env import MtgEnv
from .evalkit.policy import BasePolicy

# ── action-slot layout (mirrors crate::codec — the flat Discrete(ACTION_DIM) vocabulary) ────────────
# Only the entity-backed buckets matter to the net: their slots map 1:1 onto observation rows, so an
# action's features ARE the pointed row's features. The bases are derived from the obs table widths
# (MAX_HAND / MAX_PERM / MAX_STACK read from the encoder spec) + the fixed abstract-bucket sizes, and
# `slot_layout` asserts the total equals PyGame.action_dim() so an obs↔codec desync is caught loudly.
_N_PLAYER_SLOTS = 2
_MAX_MODES = 16
_N_COLORS = 5
_MAX_NUM = 16


def slot_layout(max_hand: int, max_perm: int, max_stack: int, action_dim: int) -> dict:
    """The ``{bucket: (base, count)}`` map + a check that it tiles ``[0, action_dim)`` exactly as
    ``codec.rs`` lays it out (COMMIT, HAND, PERM, PLAYER, STACK, MODE, COLOR, NUMBER, YES, NO)."""
    commit = 0
    hand = commit + 1
    perm = hand + max_hand
    player = perm + max_perm
    stack = player + _N_PLAYER_SLOTS
    mode = stack + max_stack
    color = mode + _MAX_MODES
    number = color + _N_COLORS
    yes = number + _MAX_NUM
    no = yes + 1
    dim = no + 1
    assert dim == action_dim, f"slot layout total {dim} != action_dim {action_dim} (obs↔codec desync)"
    # Only the entity-backed buckets are used for content scatter; the rest ride the slot embedding.
    return {"hand": (hand, max_hand), "perm": (perm, max_perm), "stack": (stack, max_stack),
            "action_dim": dim}


# ── the Q-network (action-as-input) ─────────────────────────────────────────────────────────────────
_TABLES = ("bf", "hand", "stack")
# Which entity table each action bucket points at (PERM slots point at the battlefield table).
_BUCKET_TABLE = {"perm": "bf", "hand": "hand", "stack": "stack"}


class DMCNet(nn.Module):
    """``Q(s, ·) -> (B, ACTION_DIM)`` with an action-as-input head.

    Pipeline (all shapes introspected from the obs space — nothing hard-coded):
      1. **Row encode** each entity table (bf/hand/stack): ``[feat, hashed-id embed, cardid one-hot]``
         through a per-table MLP → ``(B, R, H)`` row vectors. Feature 0 is the row-present flag (the
         encoder convention shared with ``EntityExtractor``).
      2. **State summary** (DeepSets): masked-mean-pool each table's rows, concat with ``globals`` and
         the current decision's source-card one-hot (``decision_cardid``) → an MLP → ``(B, D_state)``.
      3. **Action features**: a learned per-slot embedding ``(ACTION_DIM, E)`` gives every slot an
         identity; for HAND/PERM/STACK slots we additionally scatter the *pointed entity's row vector*
         (reused from step 1) into that slot's content channel — this is the action-as-input content.
      4. **Score**: a shared MLP over ``[state ⊕ slot-embed ⊕ action-content]`` per slot → one scalar
         Q per action. Illegal actions are masked by the caller (selection), never inside the net.
    """

    def __init__(self, obs_space, action_dim, *, id_embed=16, vocab=4096, row_hidden=64,
                 state_dim=128, slot_embed=16, q_hidden=128):
        super().__init__()
        self.action_dim = int(action_dim)
        self.vocab = int(vocab)
        self.id_embed = nn.Embedding(vocab, id_embed)

        self.row_mlps = nn.ModuleDict()
        self.cardid_dims = {}
        for name in _TABLES:
            feat_dim = obs_space[f"{name}_feat"].shape[-1]
            cid = f"{name}_cardid"
            cdim = obs_space[cid].shape[-1] if cid in obs_space.spaces else 0
            self.cardid_dims[name] = cdim
            self.row_mlps[name] = nn.Sequential(nn.Linear(feat_dim + id_embed + cdim, row_hidden),
                                                nn.ReLU())
        self.row_hidden = row_hidden

        g = obs_space["globals"].shape[0]
        self.decision_dim = (obs_space["decision_cardid"].shape[-1]
                             if "decision_cardid" in obs_space.spaces else 0)
        self.state_mlp = nn.Sequential(
            nn.Linear(g + row_hidden * len(_TABLES) + self.decision_dim, state_dim), nn.ReLU())

        self.slot_embed = nn.Embedding(action_dim, slot_embed)
        self.q_mlp = nn.Sequential(nn.Linear(state_dim + slot_embed + row_hidden, q_hidden), nn.ReLU(),
                                   nn.Linear(q_hidden, 1))

        # Table widths → action-slot layout (asserts it tiles [0, action_dim) like codec.rs).
        self.layout = slot_layout(obs_space["hand_ids"].shape[-1], obs_space["bf_ids"].shape[-1],
                                  obs_space["stack_ids"].shape[-1], action_dim)

    def _encode_rows(self, obs):
        """Per-table row vectors ``{name: (B, R, H)}`` + present masks ``{name: (B, R, 1)}``."""
        rows, present = {}, {}
        for name in _TABLES:
            feat = obs[f"{name}_feat"]                                   # (B, R, F)
            ids = (obs[f"{name}_ids"].long() % self.vocab)               # (B, R)
            parts = [feat, self.id_embed(ids)]
            if self.cardid_dims[name]:
                parts.append(obs[f"{name}_cardid"])
            rows[name] = self.row_mlps[name](torch.cat(parts, dim=-1))   # (B, R, H)
            present[name] = feat[..., :1]                                # (B, R, 1); feat 0 = present
        return rows, present

    def forward(self, obs):
        B = obs["globals"].shape[0]
        rows, present = self._encode_rows(obs)

        pooled = []
        for name in _TABLES:
            summed = (rows[name] * present[name]).sum(dim=1)             # (B, H)
            pooled.append(summed / present[name].sum(dim=1).clamp(min=1.0))
        state_in = [obs["globals"], *pooled]
        if self.decision_dim:
            state_in.append(obs["decision_cardid"].reshape(B, -1))
        state = self.state_mlp(torch.cat(state_in, dim=-1))             # (B, D_state)

        # Action-as-input content: scatter each entity row vector into the slot that points at it.
        content = torch.zeros(B, self.action_dim, self.row_hidden, device=state.device,
                              dtype=state.dtype)
        for bucket, table in _BUCKET_TABLE.items():
            base, count = self.layout[bucket]
            content[:, base:base + count, :] = rows[table][:, :count, :]

        slot = self.slot_embed.weight.unsqueeze(0).expand(B, -1, -1)    # (B, A, E)
        state_b = state.unsqueeze(1).expand(-1, self.action_dim, -1)    # (B, A, D_state)
        q = self.q_mlp(torch.cat([state_b, slot, content], dim=-1)).squeeze(-1)  # (B, A)
        return q


# ── obs plumbing ────────────────────────────────────────────────────────────────────────────────────
def stack_obs(obs_list):
    """List of per-state ``{name: ndarray}`` dicts → one ``{name: batched ndarray}`` dict."""
    return {k: np.stack([o[k] for o in obs_list]) for k in obs_list[0]}


def obs_to_tensors(stacked, device):
    """Batched-numpy obs dict → torch tensors on ``device`` (int64 for ``*_ids``/``decision_ids``)."""
    out = {}
    for k, v in stacked.items():
        t = torch.as_tensor(v)
        out[k] = t.long().to(device) if k.endswith("_ids") else t.float().to(device)
    return out


# ── ε-greedy action selection ─────────────────────────────────────────────────────────────────────
def greedy_from_q(q_row, mask_row, rng):
    """Argmax of ``q_row`` over legal actions (``mask_row``), ties broken UNIFORMLY AT RANDOM (not by
    lowest index — that would collapse an untrained net onto slot 0 = COMMIT/PASS and never play).
    ``q_row``/``mask_row`` are 1-D numpy; returns an int action."""
    legal = np.flatnonzero(mask_row)
    qv = q_row[legal]
    best = legal[qv >= qv.max() - 1e-8]
    # rng.choice works for both a np.random.Generator (collector) and the np.random module (eval, which
    # evalkit seeds globally); the single-best fast path avoids an RNG draw on the common case.
    return int(rng.choice(best)) if len(best) > 1 else int(best[0])


def select_actions(net, obs_list, mask_list, device, *, epsilon, rng):
    """ε-greedy actions for a batch of decisions (one net forward). With prob ``epsilon`` a decision
    picks a uniform random legal action; otherwise the (random-tie-broken) argmax of Q over legal."""
    with torch.no_grad():
        q = net(obs_to_tensors(stack_obs(obs_list), device)).cpu().numpy()  # (K, A)
    out = np.empty(len(mask_list), dtype=np.int64)
    for k, m in enumerate(mask_list):
        legal = np.flatnonzero(m)
        if epsilon > 0.0 and rng.random() < epsilon:
            out[k] = int(rng.choice(legal))
        else:
            out[k] = greedy_from_q(q[k], m, rng)
    return out


class EpsilonSchedule:
    """Linear ε decay from ``start`` to ``end`` over ``decay_steps`` env-steps, constant thereafter.

    A high initial ε matters here: from a random-init net the greedy policy is near-degenerate (Q≈0
    everywhere → the random tie-break is the only exploration), so we anneal exploration down as Q
    becomes informative rather than trusting a fixed tiny ε (DouZero can use ε≈0.01 because it runs
    many parallel actors off a non-collapsing action space; we start from one net and one game shape)."""

    def __init__(self, start=0.9, end=0.05, decay_steps=300_000):
        self.start = float(start)
        self.end = float(end)
        self.decay_steps = max(int(decay_steps), 1)

    def value(self, step: int) -> float:
        frac = min(max(step, 0) / self.decay_steps, 1.0)
        return self.start + (self.end - self.start) * frac


# ── Monte-Carlo return labelling ────────────────────────────────────────────────────────────────────
def compute_returns(seats, seat0_reward):
    """Terminal-return label for each transition of one finished game. ``seats`` is the acting seat
    (0/1) of each stored transition, in order; ``seat0_reward`` is the game's result from seat 0's view
    (+1 / 0 / −1). Zero-sum ⇒ seat-0 transitions get ``seat0_reward`` and seat-1 transitions its
    negation (a draw stays 0). Undiscounted: every transition in a game shares the same ±1 label."""
    r0 = float(seat0_reward)
    return [r0 if s == 0 else -r0 for s in seats]


# ── replay buffer (per-key ring, bounded memory) ────────────────────────────────────────────────────
class ReplayBuffer:
    """Fixed-capacity ring of ``(obs, action, target)`` transitions. Obs is stored per-key in
    preallocated arrays (shapes taken from a sample obs) so memory is bounded and sampling is a single
    fancy-index gather. Overwrites oldest when full."""

    def __init__(self, capacity, sample_obs, action_dtype=np.int64):
        self.capacity = int(capacity)
        self._obs = {k: np.zeros((self.capacity, *v.shape), dtype=v.dtype)
                     for k, v in sample_obs.items()}
        self._action = np.zeros(self.capacity, dtype=action_dtype)
        self._target = np.zeros(self.capacity, dtype=np.float32)
        self.size = 0
        self._pos = 0

    def add(self, obs, action, target):
        i = self._pos
        for k, v in obs.items():
            self._obs[k][i] = v
        self._action[i] = action
        self._target[i] = target
        self._pos = (i + 1) % self.capacity
        self.size = min(self.size + 1, self.capacity)

    def sample(self, batch_size, rng):
        idx = rng.integers(0, self.size, size=int(batch_size))
        obs = {k: v[idx] for k, v in self._obs.items()}
        return obs, self._action[idx], self._target[idx]


# ── mirror self-play collector ──────────────────────────────────────────────────────────────────────
class SelfPlayCollector:
    """N ``MtgEnv(opponent="external")`` games stepped in lockstep; the CURRENT net plays BOTH seats
    ε-greedy (mirror self-play). Per-seat transitions are buffered until a game ends, then labelled
    with that game's ±1 return and pushed. Auto-resets finished games and keeps collecting.

    This reuses the exact ``ext_*`` engine interface ``BatchedSelfPlayVecEnv`` is built on — advance
    every game to a decision, answer the whole batch in one net forward — but (a) drives both seats
    with the live net instead of a frozen SB3 pool and (b) records data for both seats, which the
    PPO-shaped ``BatchedSelfPlayVecEnv`` (learner-seat-only, SB3-checkpoint opponent) cannot do. Eval
    stays in evalkit; this is collection only."""

    def __init__(self, deck, num_envs, *, seed=0, max_decisions=3000):
        self.envs = [MtgEnv(deck=deck, opponent="external", max_decisions=max_decisions)
                     for _ in range(num_envs)]
        self.num_envs = num_envs
        self._rng = np.random.default_rng(seed)
        self._seed = np.random.default_rng(seed ^ 0xD1CE)
        # Per-env in-flight episode: parallel lists of (obs, action, seat).
        self._ep = [{"obs": [], "act": [], "seat": []} for _ in range(num_envs)]
        for e in self.envs:
            e.ext_reset(self._new_seed())
        self.games_done = 0
        self.env_steps = 0

    def _new_seed(self):
        return int(self._seed.integers(0, (1 << 63) - 1))

    def _flush_terminal(self, i, buffer):
        """Label the finished game i's stored transitions with the ±1 return and push them."""
        env = self.envs[i]
        r0 = env.ext_reward()                       # +1/0/−1 from seat 0's perspective
        ep = self._ep[i]
        targets = compute_returns(ep["seat"], r0)
        for o, a, t in zip(ep["obs"], ep["act"], targets):
            buffer.add(o, a, t)
        self._ep[i] = {"obs": [], "act": [], "seat": []}
        self.games_done += 1
        env.ext_reset(self._new_seed())

    def collect(self, net, buffer, device, *, min_transitions, epsilon):
        """Advance the games in batched rounds until at least ``min_transitions`` new transitions have
        been buffered; returns the count added. Each round: gather every game currently at a
        (learner-or-opponent) decision, pick actions in ONE net forward, apply, record. Terminals are
        flushed + reset inline so collection never stalls."""
        added = 0
        for _ in range(1_000_000):  # guard against a pathological non-terminating batch
            pending = []            # (env_index, seat)
            for i, env in enumerate(self.envs):
                st = env.ext_state()
                if st == "terminal":
                    self._flush_terminal(i, buffer)
                    st = env.ext_state()
                if st in ("learner", "opponent"):
                    pending.append((i, 0 if st == "learner" else 1))
            if not pending:
                continue
            obs_list = [self.envs[i].ext_obs() for (i, _) in pending]
            mask_list = [self.envs[i].ext_mask() for (i, _) in pending]
            acts = select_actions(net, obs_list, mask_list, device, epsilon=epsilon, rng=self._rng)
            for k, (i, seat) in enumerate(pending):
                ep = self._ep[i]
                ep["obs"].append(obs_list[k])
                ep["act"].append(int(acts[k]))
                ep["seat"].append(seat)
                self.envs[i].ext_apply(int(acts[k]))
                self.env_steps += 1
                added += 1
            if added >= min_transitions:
                return added
        return added


# ── evalkit Policy adapter ──────────────────────────────────────────────────────────────────────────
class DMCPolicy(BasePolicy):
    """Thin ``Policy`` adapter so a ``DMCNet`` is evaluable through the standard evalkit Arena / hooks.

    ``greedy`` = random-tie-broken argmax of Q over legal actions (the headline). ``sample`` = a
    softmax over the *legal* Q-values at temperature ``temp`` — a value method has no native action
    distribution, so this is the honest-stochastic curve evalkit's "sampled" tag wants (peaked by
    default so it tracks greedy rather than collapsing to random; ``temp`` documented in the run
    description). Stateless (per-decision), so ``reset`` no-ops via ``BasePolicy``."""

    def __init__(self, net, device="cpu", temp=0.1):
        self.net = net
        self.device = device
        self.temp = float(temp)

    def _q(self, obs_batch):
        self.net.eval()
        with torch.no_grad():
            return self.net(obs_to_tensors(stack_obs(obs_batch), self.device)).cpu().numpy()

    def act(self, obs_batch, mask_batch, *, mode="greedy", env_indices=None):
        q = self._q(obs_batch)
        out = np.empty(len(mask_batch), dtype=np.int64)
        for k, m in enumerate(mask_batch):
            m = np.asarray(m, dtype=bool)
            if mode == "greedy":
                out[k] = greedy_from_q(q[k], m, np.random)
            else:
                legal = np.flatnonzero(m)
                z = q[k][legal] / max(self.temp, 1e-6)
                p = np.exp(z - z.max())
                p /= p.sum()
                out[k] = int(np.random.choice(legal, p=p))
        return out


# ── the DMC training loop ────────────────────────────────────────────────────────────────────────────
def train_dmc(deck="heralds", *, env_steps=500_000, n_envs=64, run_name="4.3-dmc-heralds",
              tensorboard_root="/home/xander/dev/p-mtg/mtgenv/data/tb", eval_every=25_000, eval_games=200,
              buffer_capacity=60_000, batch_size=512, updates_per_iter=16, lr=1e-3,
              collect_per_iter=2048, min_buffer=5_000, eps_start=0.9, eps_end=0.05,
              eps_decay_frac=0.6, sample_temp=0.1, device=None, seed=0, notes=None,
              eval_vs_initial=True, verbose=1):
    """Run Deep Monte-Carlo self-play for ``env_steps`` applied sub-steps, logging the canonical evalkit
    battery into ``<tensorboard_root>/<run_name>`` every ``eval_every`` steps. Returns ``(net, run_dir)``.

    One "env step" = one applied engine sub-step (a learner OR opponent factored decision) — the natural
    simulator-work unit for this mirror self-play collector; each produces one stored transition."""
    import os

    import torch.nn.functional as F
    from torch.utils.tensorboard import SummaryWriter

    from .evalkit.hooks import evaluate_checkpoint
    from .evalkit.policy import RandomPolicy
    from .evalkit.scripted import ScriptedPolicy
    from .tb_meta import CUSTOM_SCALARS_LAYOUT, build_notes

    if device is None:
        device = "cuda" if torch.cuda.is_available() else "cpu"
    torch.manual_seed(seed)
    np.random.seed(seed & 0x7FFF_FFFF)

    probe = MtgEnv(deck=deck, opponent="external")
    obs_space, action_dim = probe.observation_space, probe.action_dim
    probe.ext_reset(seed)
    sample_obs = probe.ext_obs()

    net = DMCNet(obs_space, action_dim).to(device)
    initial = DMCNet(obs_space, action_dim).to(device)   # frozen random-init snapshot (vs_initial)
    initial.load_state_dict(net.state_dict())
    initial.eval()
    opt = torch.optim.Adam(net.parameters(), lr=lr)

    buffer = ReplayBuffer(buffer_capacity, sample_obs)
    collector = SelfPlayCollector(deck, n_envs, seed=seed)
    eps_sched = EpsilonSchedule(eps_start, eps_end, int(env_steps * eps_decay_frac))
    upd_rng = np.random.default_rng(seed ^ 0xA11CE)

    run_dir = os.path.join(tensorboard_root, run_name)
    os.makedirs(run_dir, exist_ok=True)
    writer = SummaryWriter(run_dir)
    config = dict(algo="DMC (Deep Monte-Carlo, DouZero-style)", deck=deck, env_steps=env_steps,
                  n_envs=n_envs, head="action-as-input", gamma="1.0 (undiscounted MC)",
                  shaping="OFF (raw ±1)", lr=lr, batch_size=batch_size, buffer_capacity=buffer_capacity,
                  updates_per_iter=updates_per_iter, collect_per_iter=collect_per_iter,
                  eps=f"{eps_start}->{eps_end} over {int(eps_decay_frac*100)}%", sample_temp=sample_temp,
                  seed=seed, device=device)
    writer.add_text("run/notes", build_notes(config, notes), 0)
    writer.add_custom_scalars(CUSTOM_SCALARS_LAYOUT)
    writer.flush()

    def do_eval(step):
        # Canonical opponents: vs random (carries stats/game/analyzers), vs the frozen random-init
        # self, and vs the scripted reference (the standing yardstick — DMC≈0.5 means it learned the
        # deck). All share the identical evalkit tag schema so DMC overlays the other arms in evaldash.
        opponents = {
            "selfplay/winrate_vs_random": RandomPolicy(seed=5_000_000),
            "selfplay/winrate_vs_script": ScriptedPolicy(),
        }
        if eval_vs_initial:
            opponents["selfplay/winrate_vs_initial"] = DMCPolicy(initial, device, sample_temp)
        res = evaluate_checkpoint(DMCPolicy(net, device, sample_temp), step=step, run_dir=run_dir,
                                  deck=deck, opponents=opponents, games=eval_games, writer=writer,
                                  algo="DMC", run_name=run_name)
        g = res["selfplay/winrate_vs_random"]["greedy"]
        s = res["selfplay/winrate_vs_random"]["sample"]
        vscr = res["selfplay/winrate_vs_script"]["greedy"]
        if verbose:
            print(f"[eval step={step}] vs_random greedy={g.win_rate:.3f} sampled={s.win_rate:.3f} "
                  f"vs_script={vscr.win_rate:.3f} turns={g.avg_turns:.1f} buffer={buffer.size}",
                  flush=True)
        return g.win_rate

    do_eval(0)  # untrained baseline (also the fast smoke that the pipeline logs)
    next_eval = eval_every
    last_loss = float("nan")
    while collector.env_steps < env_steps:
        eps = eps_sched.value(collector.env_steps)
        collector.collect(net, buffer, device, min_transitions=collect_per_iter, epsilon=eps)
        if buffer.size >= min_buffer:
            net.train()
            for _ in range(updates_per_iter):
                obs, act, tgt = buffer.sample(batch_size, upd_rng)
                q = net(obs_to_tensors(obs, device))
                qa = q.gather(1, torch.as_tensor(act, device=device).long().view(-1, 1)).squeeze(1)
                loss = F.mse_loss(qa, torch.as_tensor(tgt, device=device).float())
                opt.zero_grad()
                loss.backward()
                opt.step()
            last_loss = float(loss.item())
            writer.add_scalar("train/loss", last_loss, collector.env_steps)
            writer.add_scalar("train/epsilon", eps, collector.env_steps)
            writer.add_scalar("train/games_done", collector.games_done, collector.env_steps)
        if collector.env_steps >= next_eval:
            do_eval(collector.env_steps)
            next_eval += eval_every
            if verbose:
                print(f"  step={collector.env_steps} loss={last_loss:.4f} eps={eps:.3f} "
                      f"games={collector.games_done}", flush=True)

    do_eval(collector.env_steps)  # final-checkpoint number (the reported metric — no peak-picking)
    writer.flush()
    writer.close()
    return net, run_dir
