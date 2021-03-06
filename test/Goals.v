Theorem plus_x_y_eq_plus_y_x: forall x y, x + y = y + x.
Proof. 
  intros.
  admit.
Admitted.

Theorem plus_n_0_eq_n: forall n, 0 + n = 0.
Proof.
  intros.
  simpl.
  destruct n.
  + auto.
  + admit. (* This one is impossible *)
Admitted.


