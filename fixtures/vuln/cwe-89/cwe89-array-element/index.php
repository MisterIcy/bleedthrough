<?php

$params = ['x' => $_GET['x']];
$pdo = new PDO('sqlite::memory:');
$pdo->query('SELECT * FROM users WHERE id = ' . $params['x']);
