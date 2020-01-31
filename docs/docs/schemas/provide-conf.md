---
id: provide-conf
title: ProvideConf
sidebar_label: ProvideConf
---

The provide conf type is used inside of executors to help define what sorts of tools they provide.
These can the be matched on by a particular task inside of dev-loop.

- `name`: String [REQUIRED]

The name corresponds to the name of the thing the executor provides. This is usually
something like: `python` (or an actual command), but it doesn't have to be.
It could be anything at all.

- `version`: String [OPTIONAL]

The version of the provided tool. This should be a <a href="https://semver.org/" class="internal-link">semantic version</a>.
