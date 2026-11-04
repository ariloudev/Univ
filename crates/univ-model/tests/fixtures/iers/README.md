# IERS tables of TAI − UTC

Recorded from the IERS Earth Orientation Centre (Paris Observatory) on 2026-11-03, and read by
`tests/time_tables.rs` to check the table in `src/time/utc.rs`:

- `Leap_Second.dat`: <https://hpiers.obspm.fr/iers/bul/bulc/Leap_Second.dat>, TAI − UTC from
  1972 on, updated through IERS Bulletin C 72 (July 2026); it expires on 28 June 2027.
- `UTC-TAI.history`: <https://hpiers.obspm.fr/iers/bul/bulc/UTC-TAI.history>, TAI − UTC from
  1961, including the offset-and-drift rows of 1961–1971.

When a new Bulletin C announces a leap second or extends the expiry, record both files again
and update the table and its expiry.
