<?php

$input = $_GET['x'];
$value = $input;
$pdo = new PDO('sqlite::memory:');
$stmt = $pdo->prepare('SELECT * FROM users WHERE id = ?');
$stmt->execute([$value]);
