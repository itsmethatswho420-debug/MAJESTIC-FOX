# Aim Coaching — design doc (Majestic Fox)

**Status:** concept, approved in principle by Harry 2026-10-11 ("use that safe
for right now"). Batch with other app ideas — no solo upgrade pass.

## The idea in one line

The app watches **Harry's own stick inputs** while he plays and tells him what
* his hands* are doing wrong — a coach, not a cheat.

Plain version for Harry: *"Your stick tells on you."* Over-swinging past the
target, shaking while tracking, correcting three times before you settle —
the app sees it in your controller data and calls it out, so you can fix the
habit instead of guessing.

## The hard line (non-negotiable)

- **No screen reading. No enemy/target detection. Ever.**
- Detecting enemies on screen is aimbot technology **regardless of purpose**.
  A "coaching" label does not change what the capability is, and building it
  would put the app — and Harry's accounts — in the ban zone next to
  everything already rejected (aim-lock scripts, SNAKE_V1, RocketMod configs).
- This feature reads **controller inputs only**: stick X/Y positions over
  time, trigger/ADS state. It never looks at pixels, never knows where an
  enemy is, and cannot aim for anyone. That is what keeps it a trainer
  (like Aim Lab) instead of a cheat.

## What "input-side" means

The app records a time series of the right-stick (aim stick) position plus
context flags, sampled at a fixed rate (target: 120 Hz; 60 Hz minimum viable).
From that stream alone — no game state, no screen — we derive mechanics
metrics. The insight: most aiming flaws are visible in the *shape* of the
stick movement, not in where the bullets land.

What we log per sample: timestamp, stick X/Y (-100..100), ADS held or not,
firing or not. That's it. No video, no audio, no screen capture.

## Metrics spec

### 1. Over-correction oscillation count
- **What it is:** after a big stick movement in one direction, how many times
  the stick reverses direction before settling (small deadzone-excluded
  jitter doesn't count).
- **How computed:** detect sign changes in stick velocity on the dominant
  axis within 600 ms of a flick peak (velocity > threshold); count reversals
  above the deadzone.
- **Bad looks like:** 3+ reversals — the classic "swing past, swing back,
  swing past again" before settling.
- **Coaching line:** *"You're over-swinging — 3 corrections before you settled
  on that one. Try stopping the stick earlier."*

### 2. Flick distance consistency
- **What it is:** for repeated similar engagements, how consistent the
  initial flick distance is (first 150 ms of a flick, ADS held).
- **How computed:** cluster flicks by peak velocity; report the coefficient
  of variation of flick distance within clusters.
- **Bad looks like:** high variance — sometimes you flick 40% stick, sometimes
  90%, for what should be the same motion. Inconsistent muscle memory.
- **Coaching line:** *"Your opening flicks are all over the place — tighten
  up the first push and the corrections get smaller."*

### 3. Tracking smoothness (stick velocity variance)
- **What it is:** while ADS is held and the stick is moving (tracking, not
  flicking), how jerky the velocity is.
- **How computed:** variance of stick velocity magnitude over 500 ms sliding
  windows during sustained ADS movement; separate from flicks by velocity
  threshold.
- **Bad looks like:** high variance — stuttery, stop-start tracking instead
  of one smooth glide.
- **Coaching line:** *"Your tracking is choppy — one smooth glide beats five
  little shoves."*

### 4. Late-correction patterns
- **What it is:** corrections that start *after* firing begins — i.e., the
  habit of shooting first and adjusting second.
- **How computed:** correlate trigger-pull onset with the first significant
  stick correction; measure the delay. A pattern of fire-then-correct across
  many engagements flags the habit.
- **Bad looks like:** consistently firing 150+ ms before the first correction.
- **Coaching line:** *"You're shooting before you're settled — get the stick
  right first, then squeeze."*

## Sample input-log event schema

```json
{
  "sessionId": "2026-10-11-mw4-001",
  "game": "MW4",
  "profileId": "mw4-assault-rifle",
  "sampleRateHz": 120,
  "samples": [
    { "t": 0.000, "rx": 0, "ry": 0, "ads": false, "firing": false },
    { "t": 0.008, "rx": 12, "ry": -4, "ads": true, "firing": false },
    { "t": 0.017, "rx": 38, "ry": -9, "ads": true, "firing": true }
  ],
  "metrics": {
    "oscillationCount": { "avg": 2.3, "worst": 5 },
    "flickConsistency": { "cv": 0.31 },
    "trackingSmoothness": { "velocityVariance": 184.2 },
    "lateCorrections": { "avgDelayMs": 210, "flagged": true }
  }
}
```

- `rx`/`ry`: right-stick deflection, -100..100.
- Logs stay **local** on the Dell. Nothing uploads anywhere — Harry's inputs
  are his business, and the privacy rule ("anything he shares stays between
  us") applies to telemetry too.
- Metrics are computed from the samples; the raw log can be discarded after
  scoring unless Harry wants to keep it.

## Data sources: evaluation

Two candidate ways to get the stick stream:

| | Zen GPC-side input logging | Windows XInput polling |
|---|---|---|
| **How** | The Fox custom script records stick values to the Zen's serial output; the app reads the serial stream | App polls the controller via XInput (`XInputGetState`) at 120 Hz while playing |
| **Pros** | Sees exactly what the Zen sends (post-script values); works regardless of which controller/input path Windows sees; no extra drivers | No script changes needed; simpler app-side only; well-documented API |
| **Cons** | Requires script changes + a serial protocol; Zen serial bandwidth is limited — 120 Hz of stick data plus existing OLED traffic needs care | Sees the controller *before* the Zen processes it (pre-script values); controller must be visible to Windows (it is — the Zen presents as a controller, but confirm on the Dell) |
| **Fidelity question** | Post-script = what the game actually receives | Pre-script = what Harry's hands do — arguably *better* for coaching ("your hands", not "the script") |

**Lean:** XInput polling for v1 — no script changes, and pre-script data is
actually the right signal for coaching *his* mechanics. Revisit Zen-side
logging only if XInput proves unreliable on his rig.

**TODO:** confirm the Zen-emulated controller is visible to XInput on the
Dell during a real session before committing.

## Open questions for Harry

1. **Post-game report, live overlay, or both?** (A report card after the
   match vs. a small "3 corrections — settle earlier" whisper while playing.
   Live overlay risks distraction; report risks being ignored.)
2. **Which game first?** MW4 at launch, or current Warzone/BO6 sessions?
3. **How much history?** Keep per-session scores to show improvement over
   weeks, or session-only?
4. **Coaching voice:** plain text, or spoken lines through the TTS voice?

## Non-goals / out of scope

- Anything that reads the screen or detects enemies, players, or targets.
- Anything that moves the aim, adjusts the stick, or fires for the player.
- Anything that reacts to anti-cheat state (declined 2026-10-11 — ban-evasion).
- Uploading input logs anywhere. Local only.
