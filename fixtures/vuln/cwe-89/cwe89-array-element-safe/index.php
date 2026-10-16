<?php

$params = ['x' => $_GET['x']];
$pdo = new PDO('sqlite::memory:');
$stmt = $pdo->prepare('SELECT * FROM users WHERE id = ?');
$stmt->execute([$params['x']]);
