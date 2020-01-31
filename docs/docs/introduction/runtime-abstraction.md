---
id: runtime-abstraction
title: Abstracting the Runtime
sidebar_label: Runtime Abstraction
---

So when we say "abstracting the runtime environment" what do you think of? Maybe you think of
something along the lines of Docker when dealing with local tasks. After all this is a common
solution, but why isn't it a good enough, why does Dev-Loop even need to solve this if we
have docker?

## Why isn't Docker Enough? ##

The answer is because Docker requires Overhead. While docker is certainly a fine solution
it is really just a low level primitive of how to run containers.

Let's say you want to run a build in java without having the user to install the right version of java. What might that look like
when writing a bash script?

Perhaps something like:

```shell
docker run --rm -d -v "$(pwd):/mnt/src/" --workdir "/mnt/src/" --entrypoint "" --name "jdk-builder" openjdk:11 tail -f /dev/null || {
  status_code=$?
  echo "Failed to start container jdk-builder perhaps it's already running?"
  exit $status_code
}
docker exec -it jdk-builder /bin/bash -c "javac src/MyApp.java" || {
  status_code=$?
  docker kill jdk-builder || echo "Failed to kill jdk-builder please kill manually."
  exit $status_code
}
docker exec -it jdk-builder /bin/bash -c "jar -cvf MyJarFile.jar src/MyApp.class" || {...}
docker kill jdk-builder || {
  status_code=$?
  echo "Failed to kill jdk-builder, please kill it manually"
  exit $status_code
}
```

Look at all that code. Even if we extract the: `|| {}` each into their own specialized functions
so it becomes: `|| exitAndRemoveContainer` for example, it's still a lot of overhead.

This isn't dockers fault per say, it did do what docker set out to do. It's just a primitive. A container engine.
While it does it's job here it's not clean. Wouldn't it be great if we could just write what we wanted?:

```shell
javac src/MyApp.java
jar -cvf MyJarFile.jar src/MyApp.class
```

This is 100% possible with dev-loop. To learn more feel free to go to <a href="/docs/walkthrough/executors" class="internal-link">read aboout choosing a runtime environment</a>.
