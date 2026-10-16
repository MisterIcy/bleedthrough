<?php

function handle(string $value): void
{
    if (str_starts_with($value, '/') && !str_starts_with($value, '//')) {
        header('Location: ' . $value);
    }
}

handle($_GET['x']);
