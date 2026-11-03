# Recorded provider answers

Each provider has a directory here, such as `vizier/`, `skyserver/`, `gaia/` and `cosmicflows/`,
holding small answers recorded from the real service. Tests read them instead of the network.

Each recording sits beside a YAML file with the same stem that gives the URL or query it answers,
when it was retrieved and its SHA-256. Keep recordings to the few rows or bytes a test needs.
