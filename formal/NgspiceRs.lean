/-
Root module for the ngspice-rs formal-verification pilot.

See `formal/README.md` for the scope, the trust boundary between the proved
statements and the Rust code, and how to run the build.
-/
module
public import NgspiceRs.Horner
public import NgspiceRs.Integrator.Corrector
public import NgspiceRs.Orders
