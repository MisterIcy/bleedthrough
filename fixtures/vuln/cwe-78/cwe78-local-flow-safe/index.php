<?php

$input = $_GET['x'];
$value = $input;
system('ls -l ' . escapeshellarg($value));
