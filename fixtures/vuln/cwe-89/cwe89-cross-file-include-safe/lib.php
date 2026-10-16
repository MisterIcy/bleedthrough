<?php

function handle(string $value): void
{
    $pdo = new PDO('sqlite::memory:');
    $stmt = $pdo->prepare('SELECT * FROM users WHERE id = ?');
    $stmt->execute([$value]);
}
