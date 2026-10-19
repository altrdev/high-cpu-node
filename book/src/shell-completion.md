# Shell completion

`torchio completions <bash|zsh|fish>` prints a script that makes the shell complete the commands (`torchio me<Tab>` gives `memory`), their options (`--hold`, `--url`…) and the words `on` and `off` of `ready`. With Homebrew the completions are installed with the binary: open a new shell. Otherwise:

```
# bash: in ~/.bashrc
eval "$(torchio completions bash)"

# zsh: in ~/.zshrc, after compinit
eval "$(torchio completions zsh)"

# fish
torchio completions fish > ~/.config/fish/completions/torchio.fish
```

If Tab does nothing in zsh after `brew install`, there are two usual causes:

- The folder of the completions, `$(brew --prefix)/share/zsh/site-functions`, is not in your `fpath`. `brew shellenv` adds it, so run it before `compinit` or before Oh My Zsh (which calls `compinit`): `eval "$(brew shellenv)"`. See [Homebrew's page on shell completion](https://docs.brew.sh/Shell-Completion).
- `compinit` ignores the folders that the group can write to, and Oh My Zsh calls it with `-i`, so it does not say so. If `ls -ld $(brew --prefix)/share/zsh/site-functions` shows `drwxrwxr-x`, remove the group's write permission from that folder and its two parents, and not from the whole prefix: `chmod go-w "$(brew --prefix)/share" "$(brew --prefix)/share/zsh" "$(brew --prefix)/share/zsh/site-functions"`. Then open a new shell, and delete `~/.zcompdump*` if it still does not complete.

The scripts are generated from the commands of the binary, so they always match the version you have. On the command line of [`watch`](watch.md#keys), Tab completes the name of a command.
