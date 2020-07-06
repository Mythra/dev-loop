# Exec Playground #

Represents a playground for messing around with error messages inside of
the `exec` command.

- Error for no command:

  `${dl} exec`

- Error for unknown task:

  `${dl} exec nonexistant-text`

- Error for no task but closely named:

  `${dl} exec echooo`

- Error for internal task:

  `${dl} exec internal-echo`