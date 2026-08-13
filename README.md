# mimi

Mimi is the language learning application that anyone can edit! It's like a combination of Duolingo + Wikipedia. It's open-source under AGPLv3. If you have any contributions or suggestions feel free to open an issue or pull request.

## Building

Mimi requires Bazel 9.2.0. To build the whole project, run:

```sh
bazel build //...
```

The development servers can be started with:

```sh
# Shared credential service on port 4770, exposed to the editor container.
bazel run //mimi_auth:dev

# MediaWiki editor on port 4771 (requires config/LocalSettings.php).
bazel run //mimi_editor:dev

# Learner API on port 4772.
bazel run //mimi_backend:dev

# Learner frontend on port 4773.
bazel run //mimi_frontend:dev
```
