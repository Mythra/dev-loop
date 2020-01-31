---
id: needs-requirement
title: NeedsRequirement
sidebar_label: NeedsRequirement
---

A needs requirement defines the needs of a task. This is used for helping choose
a particular executor for a task to run in.

- `name`: String [REQUIRED]

The name of the requirement that needs to be met by a particular executor.

- `version_matcher`: String [OPTIONAL]

A particular string that corresponds to a <a href="https://devhints.io/semver" class="internal-link">Semantic Version Matcher</a>. If
this isn't provided any version will be matched.
