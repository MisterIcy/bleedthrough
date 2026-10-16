<?php

final class Handler
{
    public function handle(string $value): void
    {
        $pdo = new PDO('sqlite::memory:');
        $pdo->query('SELECT * FROM users WHERE id = ' . $value);
    }
}

(new Handler())->handle($_GET['x']);
