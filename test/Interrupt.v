(* From https://github.com/coq/coq/issues/13748 *)

Inductive uncurry_types :=
| ccons (A : Type) (rest : uncurry_types) | cnil.

Fixpoint denoteUncurried (A : uncurry_types) : Type :=
  match A with
  | cnil => unit
  | ccons A As => A * denoteUncurried As
  end.

Fixpoint denoteUncurried_rev_nounit (A : uncurry_types) : Type
  := match A with
     | cnil         => unit
     | ccons A cnil => A
     | ccons A As   => denoteUncurried_rev_nounit As * A
     end.
Fixpoint uncurry_rev_cps (A : uncurry_types)
  : denoteUncurried_rev_nounit A -> denoteUncurried A
  := match A with
     | cnil         => fun v => v
     | ccons A As
       => match As with
          | cnil => fun _ v => (v, tt)
          | As   => fun default v => default v
          end (fun '(v, a) => (a, uncurry_rev_cps As v))
     end.
