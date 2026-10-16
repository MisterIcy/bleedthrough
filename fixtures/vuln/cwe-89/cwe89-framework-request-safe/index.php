<?php

final class Request
{
    public function query(string $key): string
    {
        return $_GET[$key] ?? '';
    }
}

$request = new Request();
$pdo = new PDO('sqlite::memory:');
$stmt = $pdo->prepare('SELECT * FROM users WHERE id = ?');
$stmt->execute([$request->query('x')]);
