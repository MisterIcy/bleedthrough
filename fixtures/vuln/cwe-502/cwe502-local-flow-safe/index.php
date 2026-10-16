<?php

$input = $_GET['x'];
$value = $input;
$object = unserialize($value, ['allowed_classes' => false]);
