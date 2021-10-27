(* This should output `plus is defined` in the result buffer. *)
Definition plus (x : nat) (y : nat) : nat :=
  match x with
  | 0 => y
  | S x' => S (plus x' y)
  end.
  
(* Querying `plus 0 1` after processing the earlier statement
   should yield `plus 0 1 : nat`. *)
Check plus 0 1.
