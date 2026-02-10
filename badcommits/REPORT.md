Okay, so we recently debugged some bad commits using "azul-doc debug-regression visual". There are multiple bad commits that currently make the visual system unusable.

- The commit 8e092a2e regressed the block-positioning-complex-001 example completely (previous better commit was 72ab2a26).
- Commit f1fcf27d removed the transfer of the body background color to the root HTML background color (previous better commit was 4bacfcac).
- Commit c33e94b0 broke the "complex margin" test (previous good commit was a017dcc2).

What I can see is that we introduced the "subtree caching" for layout nodes. The reason was that we were trying to layout PDF files with 300.000 DOM nodes because every <p> item from a text was creating a separate text node. The respective commit did fix that "caching performance" issue, but broke the entire layout.

The goal is to find out WHY these commits broke and what goal they were trying to achieve, using the commit messages.
