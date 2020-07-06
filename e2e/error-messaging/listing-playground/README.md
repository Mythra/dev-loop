# Listing Playground #

This is really a "playground" for listing that exposes all of the potential
error states within the list command. The following commands can generate
errors:

Listing a non-existant task at the top level:

  - `${dl} list non-existant`

Listing a non-oneof task at the top level:

  - `${dl} list echo`

Listing an internal oneof task at the top level:

  - `${dl} list internal-oneof first`

Listing an option of a oneof that has no options:

  - `${dl} list empty-oneof first`

Listing an option that does not exist:

  - `${dl} list public-oneof third`

Listing an option that does not exist but it close to another option:

  - `${dl} list public-oneof firstt`

Listing an option that is not a oneof inside a oneof:

  - `${dl} list public-oneof first`