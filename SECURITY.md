# Security

## Reporting a vulnerability

Please report it privately, through GitHub's
[private vulnerability reporting](https://github.com/uchmk/filer/security/advisories/new),
rather than by opening an issue. An issue is public from the moment it is
filed, which tells everyone about the problem before there is anything to
update to.

Useful in a report, roughly in order of how much they help:

- what an attacker gets, and what they have to be able to do first;
- the file, path or config that triggers it, or a small one that does;
- the version, from `filer env` or the title bar, and which Windows.

There is no bounty and no guaranteed response time: this is one person's
project. Expect a reply within about a week.

## What is in scope

filer opens files it knows nothing about in order to preview them, and runs
programs named in your own config. Both are deliberate, so the interesting
question is where that goes further than intended:

- a file that, by being previewed or listed, runs code, reads something
  outside itself, or crashes in a way that is not a clean error;
- an archive that writes outside the directory it is extracted into;
- a path, filename or archive entry that escapes quoting and becomes part of a
  command;
- anything reached from a directory you only looked at, without pressing a key
  that runs something.

Not in scope, because it is the program working:

- an opener in your `yazi.toml` running what it says it runs, including
  `<Enter>` on a file whose rule names a shell command;
- `;` and `:` running what you typed;
- a `[[preview]]` rule running the command you configured;
- filer having the same access to your files that you do.

## Supported versions

The latest release. Fixes go into the next version rather than being
backported — see [releases](https://github.com/uchmk/filer/releases).

## Dependencies

`cargo audit` runs on every push and weekly against the RustSec advisory
database, so an advisory against a dependency turns a check red rather than
waiting to be noticed.
