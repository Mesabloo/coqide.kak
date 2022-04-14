Fact something x y : S x + y = S (x + y).
Proof.
  {{{ reflexivity. }}}
Qed.

Fact identity {A} (x : A) : A.
Proof. exact x. Qed.
