# Getting your Majestic Fox installer — the easy way

Your Dell stays clean. The app gets built on a computer online, and you just
download the finished installer. You will never install build tools or
compile anything yourself.

## One-time setup (about 10 minutes)

**Step 1 — Make a free GitHub account.**
Go to github.com and sign up. It's free. Pick any username.

**Step 2 — Make a new empty repo called `majestic-fox`.**
Once you're signed in, hit the `+` in the top-right corner, then "New
repository". Name it exactly `majestic-fox`. You can leave it Private (only
you can see it) — that works fine. Don't check any of the setup boxes
(README, gitignore, license) — leave it completely empty, then hit "Create
repository".

**Step 3 — Tell Muse your GitHub username.**
Just say "my GitHub is ___" in chat. Muse will hand you the exact commands
to put the app's code into your repo — it's copy-paste, two or three lines,
done once.

## Getting a new build (every time there's an update)

**Step 4 — Tag it and download.**
When Muse says a new version is ready, you'll run two copy-paste lines that
tag it (something like `v0.1.0`) and push the tag. That's the whole trigger —
the online build starts by itself and takes about 10–20 minutes.

Then go to your repo page on GitHub, click **Releases** (right side of the
page), open the newest release, and download the file ending in
`-setup.exe`. Run it. Done.

## What you NEVER have to do

- Install Rust, Node, or Visual Studio build tools — none of it touches your Dell.
- Compile anything or run any build command.
- Worry about your Dell overheating from a build — builds happen online, your
  Dell just downloads the finished file like any other download.

If anything on this page is confusing, ask Muse in chat — that's what it's for.
