<?php

$input = $_GET['x'];
$value = $input;
readfile('/var/www/files/' . basename($value));
