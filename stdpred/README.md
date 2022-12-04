# Anoma Standard Predicates Library

A set of general purpose predicates that are defined at genesis time. They cover most common operations and can be used to construct intents and account validity predicates.

## Predicates

#### Id
  - `constant`

#### Arithmetic:
  - `uint_equal`
  - `uint_greater_than`
  - `uint_greater_than_equal`
  - `uint_greater_than_by`
  - `uint_less_than`
  - `uint_less_than_equal`
  - `uint_less_than_by`

#### Signature:
  - `verify_ed25519_signature`