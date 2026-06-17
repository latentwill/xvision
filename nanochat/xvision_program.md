# xvision_program.md — Autoresearcher agent instructions

> **Operator-editable.** This file is read by the autoresearcher agent at the
> start of every experiment iteration. Edit it to guide the research direction.
> Keep edits concise — the agent reads the full file each time.

## Goal

Improve the nanochat filter model's **val_acc** (confidence-weighted direction
accuracy) on the held-out validation set, subject to the wall-clock budget in
the run config.

`val_acc = sum(confidence_i × correct_i) / N` across held-out samples.
Higher is better. The target metric is printed as the last stdout line of
`xvision_train.py` in the form:

```
XVN_RESULT {"val_acc": <float>, "val_loss": <float>}
```

## What you may change

You may freely edit **`xvision_train.py`** only. Specifically:

- Model architecture (add layers, attention, positional encodings, depth/width).
- Optimizer and learning-rate schedule.
- Input channels: add RSI, ATR, or other indicator scalars after the OHLCV
  window. If you add indicators, add them in the same order listed in
  `input_spec.json` (which you may also update to register the new channels).
- Batch size, weight decay, dropout.
- Window size (`window_bars` in the run config; update `input_spec.json` if
  you change it).
- The conditioning-token prepend (encoding of the upstream LLM filter direction).

## What you must keep unchanged

- **`xvision_prepare.py`** — fixed harness. Never modify it.
- **The XVN_RESULT contract.** The last line of stdout must always be:
  `XVN_RESULT {"val_acc": <float>, "val_loss": <float>}`.
  Any missing or malformed final line is recorded as a crash (val_acc = NULL).
- **`safetensors` format.** The checkpoint saved by `xvision_train.py` must
  be written with `safetensors.torch.save_file`, never with `torch.save` in
  pickle mode. The Rust inference harness loads only safetensors.
- **Device selection order.** Always `mps` → `cuda` → `cpu`. Use the
  `select_device()` helper from the top of `xvision_train.py`.
- **Wall-clock budget.** Respect `train_wall_clock_sec` from the run config.
  Do not remove the deadline check; overrunning the budget causes the harness
  to SIGKILL the process and record a crash.
- **Input path contract.** Load training data from
  `run_config["output_dir"]/train.pt` and `val.pt` (produced by
  `xvision_prepare.py`). Each `.pt` file contains `{"X": Tensor, "y": Tensor}`.

## Experiment loop

Each iteration the harness:

1. Reads this file and the current `results.tsv` (a TSV of past experiments:
   `experiment_id, val_acc, val_loss, status, description`).
2. Asks the agent (you) to propose and apply one targeted change to
   `xvision_train.py`.
3. Commits the change in the training worktree.
4. Runs `uv run xvision_train.py <run_config.json>` with the wall-clock budget.
5. Reads the `XVN_RESULT` line and records the metrics.
6. If `val_acc` improved → keeps the commit. Else → `git reset` (reverts your
   change) and the baseline is restored.
7. Loops.

## Strategy suggestions (operator can edit this section)

The following experiments are proposed in priority order. Strike through each
after attempting it, and add a note on the result so future iterations have
context.

1. Add a single Transformer encoder layer (4 heads, d_model=64) before the
   linear head.
2. Add a positional embedding for the OHLCV time steps.
3. Prepend the LLM-filter conditioning token (LONG=0, SHORT=1, NEUTRAL=2,
   PASS=3) as a learned embedding before the OHLCV sequence.
4. Add RSI-14 as an additional scalar channel after the OHLCV window (update
   `input_spec.json` to register `rsi_14`).
5. Add a cosine-with-warmup learning rate schedule (100 warmup steps).
6. Try label smoothing (0.1) on the cross-entropy loss.
7. Increase `window_bars` from 64 to 128 (update `input_spec.json`).

## Forbidden

- Hardcoding `val_acc` in the output (the harness validates against the actual
  held-out set in `xvision_prepare.py`).
- Deleting or skipping the wall-clock deadline check.
- Saving checkpoints with `torch.save(..., pickle_module=...)` or any
  non-safetensors format.
- Importing or installing packages not in `pyproject.toml` without adding them
  to `pyproject.toml` first (uv will fail the install and the run crashes).
- Modifying `xvision_prepare.py`, `xvision_program.md`, or
  `run_config.json`.

## Notes for the operator

- Edit the **Strategy suggestions** section above to steer the research.
- Add a **Results log** section below as experiments run, to give the agent
  cumulative context across iterations.
- The `results.tsv` file in the worktree contains all past metrics; the agent
  reads it automatically — you do not need to copy it here.
