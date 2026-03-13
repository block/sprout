INSERT IGNORE INTO channels (id, name, channel_type, visibility, description, created_by, created_at, updated_at)
VALUES (
  UNHEX(REPLACE('9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50', '-', '')),
  'general-seeded',
  'stream',
  'open',
  '',
  UNHEX('0b5c83782cf123e698131ac976179f8366224e03db932c9da0074512aed2388d'),
  NOW(),
  NOW()
);
