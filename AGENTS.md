# Agent instructions

These apply to every agent and person working in this repository.

## Data stays out of the repository

The repository holds code, prose and the small files that code reads.
Experiment results, raw run data and device records stay on the owner's
workstation, and documents summarize them in prose. Private data is local only;
there is no private repository for it.

- Under `docs/`, a data file (JSON, CSV, text, HTML, a compressed archive and
  the like) stays under 256 KiB unless code reads it. Markdown pages and
  screenshots are not data files. CI runs `.github/ci/check_docs_data.py` on
  every change.
- Nothing under `docs/` names a device: no serial numbers, MAC addresses, SSH
  keys or Wi-Fi credentials. Hostnames such as `sw-picade-2` are fine.
- A file that code reads is the one exception, and its pull request says which
  code reads it. When a change seems to need another committed data file, raise
  it with the owner first.
- `docs/data` keeps the files code already reads and small summaries of the
  last few experiments. Delete old results rather than adding to them.
  `.github/ci/docs-data-grandfathered.txt` lists the oversized files that
  predate this rule, and it only shrinks.
- OTA run records (`ota-<subject>-<date>.json`) live outside the repository.
  The OTA documents name them so the owner can find them locally.
