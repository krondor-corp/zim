-- Which sign-in provider an account belongs to, and its stable id there.
--
-- No account linking: an account signs in through exactly one provider
-- (`ZIM_HUB_AUTH` name: 'google', 'dev-trust'). The same email arriving
-- through a different provider is refused, and a Google sign-in must
-- carry the `sub` recorded for the account. Every existing row came from
-- Google; their `auth_subject` is recorded on next sign-in.

ALTER TABLE users ADD COLUMN auth_provider TEXT NOT NULL DEFAULT 'google';
ALTER TABLE users ADD COLUMN auth_subject TEXT;
