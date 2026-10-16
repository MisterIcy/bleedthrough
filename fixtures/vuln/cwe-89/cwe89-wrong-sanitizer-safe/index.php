<?php

$value = (int) $_GET['x'];
$pdo = new PDO('sqlite::memory:');
$pdo->query('SELECT * FROM users WHERE id = ' . $value);
