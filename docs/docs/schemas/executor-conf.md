---
id: executor-conf
title: ExecutorConf
sidebar_label: ExecutorConf
---

The executor is what provides a particular runtime environment for dev-loop.
There are multiple types of them, but they are what allows running your
code inside of a docker container with no changes to anything but
configuration.

- `type`: String [REQUIRED]

The type of executor defines what particular type of executor to use.
Currently there are two types of executors supported in dev-loop:
`docker`, and `host`.

`host` runs on the host system, and is no different than actually
running a command locally (or a script for that matter).

`docker` runs a particular command inside of a docker container.
This will spin up a container for each command run (but will reuse
incase of something like a pipeline, or running multiple tasks
at once).

`docker` executors currently require containers that have:

- `bash`
- `/usr/bin/env`

- `params`: Map[String, String] [OPTIONAL]

Params contain a list of parameters in order to pass into a particular
executor. What these are depends on the executor type, so we will
describe what they are for each executor below.

***Host Executor***

The Host Executor has no possible arguments. It ignores all possible options.

***Docker Executor***

| Name                           | Type                                       | Description of Value                                                                                                                                                                                                            |
|--------------------------------|--------------------------------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| user                           | String [OPTIONAL]                          | the user to launch commands as in the container, defaults to root.                                                                                                                                                              |
| name_prefix                    | String [REQUIRED]                          | the prefix of the container to use. this is required, and used to help derive the container name which follows a format like: `dl-${name_prefix}${data}`. As such your name prefix should end with: `-`.                        |
| image                          | String [REQUIRED]                          | the docker image to use for this container. This is required. This should be a full pullable image. For example: `ubuntu:18.04`, or `gcr.io/....:latest`.                                                                       |
| extra_mounts                   | Comma Seperated String [OPTIONAL]          | a list of extra directories to mount for the docker executor. it should be noted the root project directory, and $TMPDIR will always be mounted.                                                                                |
| hostname                       | String [OPTIONAL]                          | the hostname to use for the docker container. If you don't provide one, it will be derived automatically for you. This is almost always preferred since dev-loop will ensure there are no possible conflicts.                   |
| export_env                     | Comma Seperated String [OPTIONAL]          | a comma seperated list of environment variables to allow to be passed into the container.                                                                                                                                       |
| tcp_ports_to_expose            | Comma Seperated String [OPTIONAL]          | a comma seperated list of ports to export to the host machine. you won't need to set these if you're using two tasks in a pipeline, as each pipeline gets it's own docker network that allows services to natively communicate. |
| udp_ports_to_expose            | Comma Seperated String [OPTIONAL]          | the same as `tcp_ports_to_export` just for udp instead.                                                                                                                                                                         |
| experimental_permission_helper | String'd Boolean [OPTIONAL] [EXPERIMENTAL] | [EXPERIMENTAL] will break in a later update, a flag that tells dev-loop to fix permissions on linux hosts for it's mounted volumes.                                                                                             |

- `provides`: List[<a href="/docs/schemas/provide-conf" class="internal-link">ProvideConf</a>] [OPTIONAL]

A list of things this particular executor provides. See ProvideConf for more information.
