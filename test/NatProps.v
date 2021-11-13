Fixpoint eqb (n m : nat) : bool :=
  match n with
  | O => match m with
         | O => true
         | S _ => false
         end
  | S n' => match m with
            | O => false
            | S m' => eqb n' m'
            end
  end.

Notation "x =? y" := (eqb x y) (at level 70) : nat_scope.

Theorem plus_1_neq_0_firsttry : forall n : nat, ((n + 1) =? 0) = false.
Proof.
  intros n.
  destruct n as [| n' ] eqn:E. 
  - reflexivity.
  - reflexivity.
Qed.

Theorem plus_0_n : forall n : nat, 0 + n = n.
Proof. intros n. simpl. reflexivity. Qed.
Theorem plus_1_l : forall n : nat, 1 + n = S n.
Proof. intros n. reflexivity. Qed.
Theorem mult_0_l : forall n : nat, 0 * n = 0.
Proof. intros n. reflexivity. Qed.
Theorem plus_id_example : forall n m : nat, n = m -> n + n = m + m.
Proof. intros n m. intros H. rewrite -> H. reflexivity. Qed.

(* Theorem plus_id_exercise : forall n m o : nat, n = m -> m = o -> n + m = m + o.
Proof.
  intros n m o.
  intros H.
  rewrite -> H.
  intros I.
  rewrite -> I.
  reflexivity.
Qed. *)

