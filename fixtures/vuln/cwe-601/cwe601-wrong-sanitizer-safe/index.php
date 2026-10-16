<?php

$value = htmlspecialchars($_GET['x']);
if (str_starts_with($value, '/') && !str_starts_with($value, '//')) {
    header('Location: ' . $value);
}
