<?php

$input = $_GET['x'];
$value = $input;
$pdo = new PDO('sqlite::memory:');
$pdo->query('SELECT * FROM users WHERE id = ' . $value);
