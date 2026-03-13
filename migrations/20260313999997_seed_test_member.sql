INSERT IGNORE INTO channel_members (channel_id, pubkey, role, joined_at)
VALUES (
  UNHEX(REPLACE('9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50', '-', '')),
  UNHEX('0b5c83782cf123e698131ac976179f8366224e03db932c9da0074512aed2388d'),
  'owner',
  NOW()
);
