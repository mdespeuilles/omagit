# docs/notes/

One file per dependency, recording what the **resolved** version was verified to
do — not what its README claims (SPEC §3 rule 1). Versions of `gpui-kit`,
`gpui-omarchy` and `gitoxide` move fast, and a capability that reads as obvious
is not one that exists.

Re-read and update these before each milestone. When a behaviour is uncertain,
the answer is a test that proves it, not an assumption.
