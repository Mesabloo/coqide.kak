Require Import Lia.

Theorem useless : exists (x n : nat), n*x = 0.
Proof.
  eexists _, 0.
  lia.
  Unshelve.
  all: admit.
Admitted.
