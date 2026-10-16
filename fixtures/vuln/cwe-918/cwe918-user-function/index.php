<?php

function handle(string $value): void
{
    $body = file_get_contents($value);
}

handle($_GET['x']);
