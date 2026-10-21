WITH threshold AS (
    SELECT COALESCE(MIN(elapsed_cycles)/3, ?4) AS minimum
    FROM recordings WHERE pack_name=?1 AND level_number=?2 AND outcome='win'
), ranked AS (
    SELECT started_at, outcome, elapsed_cycles,
        ROW_NUMBER() OVER (
            PARTITION BY session_started_at
            ORDER BY (outcome='win') DESC, gold_collected DESC, elapsed_cycles DESC, started_at DESC
        ) AS session_rank
    FROM recordings, threshold WHERE pack_name=?1 AND level_number=?2
        AND (outcome='win' OR elapsed_cycles>=minimum)
), best AS (
    SELECT started_at FROM recordings
    WHERE pack_name=?1 AND level_number=?2 AND outcome='win'
    ORDER BY elapsed_cycles, started_at DESC LIMIT 1
), newest AS (
    SELECT MAX(started_at) AS started_at FROM recordings
    WHERE pack_name=?1 AND level_number=?2 AND outcome='win'
)
DELETE FROM recordings
WHERE pack_name=?1 AND level_number=?2 AND started_at NOT IN (
    SELECT started_at FROM best
    UNION SELECT started_at FROM newest WHERE started_at IS NOT NULL
    UNION SELECT started_at FROM ranked, threshold
        WHERE (outcome='win' OR elapsed_cycles>=minimum)
        AND (started_at>=?3-2592000000000 OR (session_rank=1 AND started_at>=?3-7776000000000))
);
