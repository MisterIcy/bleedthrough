<?php

function handle(string $value): void
{
    $pdo = new PDO('sqlite::memory:');
    $pdo->query('SELECT * FROM users WHERE id = ' . $value);
}

handle($_GET['x']);
