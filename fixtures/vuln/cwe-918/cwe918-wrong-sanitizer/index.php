<?php

$value = filter_var($_GET['x'], FILTER_VALIDATE_URL);
$body = file_get_contents($value);
