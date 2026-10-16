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
$pdo->query('SELECT * FROM users WHERE id = ' . $request->query('x'));
