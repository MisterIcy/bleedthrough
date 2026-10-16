<?php

$input = $_GET['x'];
$value = $input;
if (str_starts_with($value, '/') && !str_starts_with($value, '//')) {
    header('Location: ' . $value);
}
