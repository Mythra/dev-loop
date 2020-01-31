# E2E #

E2E tests are a simple way for us to verify that the complete execution,
and run cycles are doing everything they should. This is more specifically
for things that aren't easily mocked like an HTTP Endpoint that's remote,
or a filesystem.

Basically we want to test that when we run multiple docker containers,
everything doesn't crash and burn. It can also be nice to validate each
configuration item in terms of how it executes, but things like http validation
are hard to fully test.

## Exec Tests ##

The following tests are run for the `exec` command:

  * `simple` - just run a simple task, it does not do anything.
  * `host-docker-passoff` - test writing to a file on the host, writing to it more inside a docker container, and then reading it from the host. this validates state natively can be passed between the two.
  * `nested-oneof` - test that you can invoke a nested oneof task
  * `reuse-container` - validate that containers are reused
  * `nested-pipelines` - test that pipelines in pipelines execute in the correct order even after crossing the host/docker boundary.
  * `port-exposing` - test that ports are exposed over tcp/udp this requires netcat installed to run.

## Run Tests ##

The tests for run command are ensuring things run in parallel even across docker and host.
This requires at least two cores. Once the test runs you need to ensure that the output of
the file `build/run/state` is:

```text
1
1
2
2
3
3
```

This ensures each task is executed at the same time.
