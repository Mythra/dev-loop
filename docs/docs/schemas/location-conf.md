---
id: location-conf
title: LocationConf
sidebar_label: LocationConf
---

A Location represents a particular location that can be fetched. This
can represent multiple things like a path on the filesystem, and/or
a remote path.

- `type`: String [REQUIRED]

Represents the type of location this is. The valid values right now are: `http`, and `path`. `http` representing
an http endpoint, and `path` representing a path on the filesystem.

- `at`: String [REQUIRED]

The actual location of the path. For `http` this should be an actual http endpoint to the file.
For `path` this should be a path relative to either the root of the repo, or the actual file
referencing where the location is.

- `recurse`: Boolean [OPTIONAL]

Whether or not to recursively look at a folder. This only applies to folders, of the `path` type.
