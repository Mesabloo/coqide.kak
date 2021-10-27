Notation "x && y" := (x + y).

(* This one should throw an error complaining that `add` is not found 
   in the current environment *)
Notation "x ++ y" := (add x y).
